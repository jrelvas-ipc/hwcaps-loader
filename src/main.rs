/*
 * Copyright (C) 2024 José Relvas.
 *
 * This program is free software; you can redistribute it and/or
 * modify it under the terms of the GNU General Public License as
 * published by the Free Software Foundation; either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful, but
 * WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
 * General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program; if not, see <http://www.gnu.org/licenses/>.
 *
 * Written by:
 *     José Relvas <josemonsantorelvas@gmail.com>
 */

#![no_std]
#![no_main]
//#![feature(lang_items)]
//#![feature(c_size_t)]
//#![feature(str_from_raw_parts)]

#![cfg_attr(target_os="none", feature(naked_functions))]

mod sys;
mod capabilities;
mod logging;

mod init;

use core::str;
use core::ffi::c_char;
use core::ffi::CStr;
use core::slice;

use memchr::{memchr, memrchr};
use logging::PrintBuff;

mod exit_code {
    macro_rules! def {
        ($name:ident, $value:expr) => {
            pub const $name: i32 = $value;
        };
    }

    def!(RUST_PANIC, -70100);
    def!(SELF_EXECUTION, -70200);
    def!(COMMAND_PATH_INVALID, -70210);
    def!(PROC_PATH_IO_ERROR, -70220);
    def!(PROC_PATH_EMPTY, -70221);
    def!(PROC_PATH_NO_PARENT, -70222);
    def!(PROC_PATH_NO_GRANDPARENT, -70223);
    def!(PATH_RESOLUTION_IO_ERROR, -70230);
    def!(TARGET_PATH_INVALID, -70240);
    def!(TARGET_PATH_TOO_LARGE, -70241);
    def!(TARGET_EXECUTION_ERROR, -70242);
    def!(TARGET_NO_VIABLE_BINARIES, -70243);
}

fn get_arg_string(ptr: *const c_char) -> &'static str {
    // argv0 can technically be larger than this, but any value which is larger
    // than a path is worthless to us anyways!
    let arg_slice = unsafe { slice::from_raw_parts(ptr as *mut u8, sys::MAX_PATH_LEN as usize) };

    let terminator_index = match memchr(b'\0', &arg_slice) {
        Some(i) => i,
        _ => abort!(exit_code::COMMAND_PATH_INVALID, "Command path: Invalid!")
    };

    return unsafe { str::from_utf8_unchecked(&arg_slice[..terminator_index+1])};

}

fn get_loader_path(buffer: &mut [u8]) -> (usize, usize, usize) {
    let loader_size = match sys::readlink(c"/proc/self/exe", buffer) {
        Ok(p) => p,
        Err(e) => abort!(exit_code::PROC_PATH_IO_ERROR, "Loader path: IO Error! ({})", e.into_raw())
    };

    if !(loader_size > 0 ){
        abort!(exit_code::PROC_PATH_EMPTY, "Loader path: Empty!")
    }

    let last_dash = match memrchr(b'/', &buffer) {
        Some(i) => i,
        _ => abort!(exit_code::PROC_PATH_NO_PARENT, "Loader path: Invalid ancestry!")
    };

    let second_last_dash = match memrchr(b'/', &buffer[..last_dash]) {
        Some(i) => i,
        _ => abort!(exit_code::PROC_PATH_NO_GRANDPARENT, "Loader path: Invalid ancestry!")
    };

    (loader_size, last_dash, second_last_dash)
}

fn resolve_path(cwd_fd: i32, path: &str, buffer: &mut [u8]) -> usize {
    let str_ptr = path.as_ptr() as *const i8;
    let c_str = unsafe { CStr::from_ptr(str_ptr) };

    let fd = match sys::openat(cwd_fd, c_str, sys::O_PATH | sys::O_NOFOLLOW) {
        Ok(d) => d,
        Err(e) => abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path resolution: IO error for \"{}\"! ({})", &path, e.into_raw())
    };

    // There are three default FDs on Linux: 0 (STDOUT); 1 (STDIN); 2 (STDERR)
    // Since we only ever open a single file descriptor in hwcaps-loader, it's usual for the FD to be 3...
    // ...unless the program which executed us didn't close its FDs...
    // use a fast path for fd 3, while including a formatting fallback for other FDs.
    let mut path_buffer;
    let path = if fd == 3 {
        "/dev/fd/3\0"
    } else {
        path_buffer = [0; 1024];
        let mut writer = PrintBuff::new(&mut path_buffer);

        _ = tfmt::uwrite!(&mut writer, "/dev/fd/{}\0", fd);
        unsafe { str::from_utf8_unchecked(&path_buffer) }
    };

    match sys::readlink(unsafe {CStr::from_bytes_with_nul_unchecked(path.as_bytes())}, buffer) {
        Ok(p) => p,
        //Err(e) => abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path Resolution error! Failed to get path of FD \"{}\"! (errno: {})", fd, e.into_raw())
        Err(e) => abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path resolution: IO error for \"{}\"! ({})", path, e.into_raw())
    }
}

#[no_mangle]
pub extern fn main(_argc: i32, argv: *const *const c_char, envp: *const *const c_char) -> ! {
    // We cheat here - argv0 and loader_path have a null terminator
    // (makes it easier to interface with C code without useless copies)
    // Modern linux kernels guarantee argv0's existence, so no need to check if the pointer is null
    let argv0 = get_arg_string(unsafe { *argv });

    let mut loader_path = [0; sys::MAX_PATH_LEN as usize];
    let (loader_len, bin_index, usr_index) = get_loader_path(&mut loader_path);

    //Make sure we're not trying to execute ourselves!
    #[cfg(feature = "self_execution_check")]
    if argv0.as_bytes().ends_with(&loader_path[bin_index+1..loader_len+1]){
        abort!(exit_code::SELF_EXECUTION, "Recursion error! Do not run the loader directly!")
    }

    let mut cwd = sys::AT_FDCWD;

    // When argv0 is a command alias (foo -> /usr/bin/foo, for example)
    // Set cwd to our binary's parent (normally /usr/bin)
    if !argv0.starts_with("/") && !argv0.starts_with("./") && !argv0.starts_with("../") {
        //Sneakily put a null byte here without making a new string
        let byte = loader_path[bin_index + 1];
        loader_path[bin_index + 1] = b'\0';

        let c_str = unsafe { CStr::from_bytes_with_nul_unchecked(&loader_path) };

        cwd = match sys::openat(sys::AT_FDCWD, c_str, sys::O_PATH) {
            Ok(d) => d,
            Err(e) => abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path Resolution: IO error while determining cwd! ({})", e.into_raw())
        };
        //Restore the previous character
        loader_path[bin_index + 1] = byte;
    }

    let mut buffer = [0; sys::MAX_PATH_LEN as usize];
    let argv0_len = resolve_path(cwd, argv0, &mut buffer);
    let argv0 = buffer;

    let first_half = &argv0[..usr_index+1];
    let second_half = &argv0[usr_index..argv0_len];

    // Check if our target's on a valid location
    if first_half != &loader_path[..usr_index + 1] {
        abort!(exit_code::TARGET_PATH_INVALID, "Target: Invalid location!")
    }

    // Prepare execution target path
    // TODO: Determine capabilities/featureset of CPU and choose the featureset directory based on that.
    let hwcaps_dir = b"hwcaps/";

    let base_length = first_half.len() + hwcaps_dir.len();
    if base_length > sys::MAX_PATH_LEN as usize {
        abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target: Path too large!")
    }

    // Very hacky and unsafe code :)
    // We can reuse the string we already have instead of allocating a new one, saving on time.
    let mut target_path = loader_path;

    // We've already determined the path starts with this, so we can just skip over that
    let start = usr_index+1;
    let end = start+hwcaps_dir.len();
    target_path[start..end].copy_from_slice(hwcaps_dir);
    let start = end;

    let mut must_format_arch = true;
    let mut version_char_index: usize = 0;
    #[allow(unused_assignments)]
    let mut path_len = 0;

    // Determine the maximum feature level supported by this machine
    let feature_level = capabilities::get_max_feature_level();

    // Generate a path for every available feature level, then attempt to execute it.
    // Repeat until execve() is sucessful or we run out of levels.
    for i in (0..=feature_level).rev() {

        if capabilities::arch_name_changed(i) {
            must_format_arch = true;
        }

        // Format the second part of the path, which is dependent on the arch name.
        if must_format_arch {
            let (relative_char_index, arch_name_len) = match capabilities::format_arch_name(&mut target_path[end..], i) {
                Ok(v) => v,
                Err(_) => abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target: Path too large!"),
            };
            version_char_index = relative_char_index + end;

            path_len = base_length + arch_name_len + second_half.len() + 1;
            if path_len > sys::MAX_PATH_LEN as usize {
                abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target: Path too large!")
            }

            let start = start + arch_name_len;
            let end = start + second_half.len();
            target_path[start..end].copy_from_slice(&second_half);
            target_path[end]= b'\0';

            must_format_arch = false;
        }

        // Unless the arch name changes, all we need to do is update the character representing the arch version.
        target_path[version_char_index] = capabilities::HWCAPS_CHARS[i as usize];

        #[cfg(debug_assertions)]
        {
            let path_string = unsafe {
                let slice = slice::from_raw_parts(target_path.as_ptr(), path_len);
                str::from_utf8_unchecked(slice)
            };
            print!("(DEBUG) Target: Executing: {}\n", path_string);
        }

        let str_ptr = target_path.as_ptr() as *const i8;
        let c_str = unsafe { CStr::from_ptr(str_ptr) };

        match sys::execve(c_str, argv, envp).into_raw() {
            sys::ENOENT => continue,
            other => {
                let path_string = unsafe {
                    let slice = slice::from_raw_parts(target_path.as_ptr(), end);
                    str::from_utf8_unchecked(slice)
                };
                abort!(exit_code::TARGET_EXECUTION_ERROR, "Target: Execution error for \"{}\"! ({})", path_string, other)

                //TODO: Use this when https://github.com/rust-lang/rust/issues/119206 is stabilized
                //abort!("Failed to execute program \"{}\"! (errno: {})", unsafe {str::from_raw_parts(target_path.as_ptr(), end)}, e)
            }
        };
    }

    abort!(exit_code::TARGET_NO_VIABLE_BINARIES, "Target: No viable binaries for loading... Is the target program installed properly?")
}
