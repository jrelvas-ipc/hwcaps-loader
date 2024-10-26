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
use core::cmp;

use logging::PrintBuff;

mod exit_code {
    macro_rules! def {
        ($name:ident, $value:expr) => {
            pub const $name: i32 = $value;
        };
    }

    def!(RUST_PANIC, 100);
    def!(SELF_EXECUTION, 200);
    def!(COMMAND_PATH_INVALID, 210);
    def!(PROC_PATH_IO_ERROR, 220);
    def!(PROC_PATH_INVALID, 221);
    def!(PATH_RESOLUTION_IO_ERROR, 230);
    def!(TARGET_PATH_INVALID, 240);
    def!(TARGET_PATH_TOO_LARGE, 241);
    def!(TARGET_EXECUTION_ERROR, 242);
    def!(TARGET_NO_VIABLE_BINARIES, 243);
}

static HWCAPS_PATH: &'static [u8] = b"/usr/hwcaps/";
//const USR_PATH: &'static [u8] = &HWCAPS_PATH[..4];
static BIN_PATH: &'static [u8] = b"/usr/bin/";

fn extract_argv0(ptr: *const *const c_char) -> &'static [u8] {
    let argv0 = unsafe {
        let ptr = *ptr; // Modern linux kernels guarantee argv0's existence, so no need to check if the pointer is null

        // from_ptr uses strlen() internally, provided by libc or as a compiler langite
        CStr::from_ptr(ptr).to_bytes_with_nul()
    };

    if argv0.len() > sys::MAX_PATH_LEN as usize || argv0.len() < 1 {
        abort!(exit_code::COMMAND_PATH_INVALID, "Command path: Invalid!")
    }

    argv0
}

fn get_loader_path(buffer: &mut [u8]) -> usize {
    let loader_size = match sys::readlink(c"/proc/self/exe", buffer) {
        Ok(p) => p,
        Err(e) => abort!(exit_code::PROC_PATH_IO_ERROR, "Loader path: IO Error! ({})", e.into_raw())
    };

    // It's safe to do this because the buffers passed to this function are always 4096 bytes
    if unsafe { buffer.get_unchecked(..BIN_PATH.len()) } != BIN_PATH {
        abort!(exit_code::PROC_PATH_INVALID, "Loader path: Invalid location!")
    }

    loader_size
}

fn resolve_path(cwd_fd: i32, path: &[u8], buffer: &mut [u8]) -> usize {
    let c_str = unsafe {
        let str_ptr = path.as_ptr() as *const i8;
        CStr::from_ptr(str_ptr)
    };

    let fd = match sys::openat(cwd_fd, c_str, sys::O_PATH | sys::O_NOFOLLOW) {
        Ok(d) => d,
        Err(e) => {
            let s = unsafe { str::from_utf8_unchecked(path) };
            abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path resolution: IO error for \"{}\"! ({})", s, e.into_raw())
        }
    };

    let mut absolute_path = make_uninit_array!(128);
    let mut writer = PrintBuff::new(&mut absolute_path);
    _ = tfmt::uwrite!(&mut writer, "/dev/fd/{}\0", fd);
    let c_str = unsafe { CStr::from_ptr(absolute_path.as_ptr()  as *const i8) };

    match sys::readlink(c_str, buffer) {
        Ok(p) => p,
        Err(e) => {
            let s = unsafe { str::from_utf8_unchecked(path) };
            abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path resolution: IO error for \"{}\"! ({})", s, e.into_raw())
        }
    }
}

// Returns -1 if path is alias
// Returns 0 if path starts with "/" (absolute)
// Returns 1 if path starts with "./" (relative)
// Returns 2 if path starts with "../" (relative)
fn get_path_kind(path: &[u8]) -> i8 {
    let last = cmp::min(path.len()-1, 2);

    for i in 0..last {
        let byte = unsafe { *path.get_unchecked(i) };

        if byte == b'/' { return i as i8 };
        if byte != b'.' { break };
    }
    -1
}

pub extern fn main(_argc: i32, argv: *const *const c_char, envp: *const *const c_char) -> ! {
    //Workaround for rust not supporting static declaration from other statics
    #[allow(non_snake_case)]
    let USR_PATH: &'static [u8] = unsafe { slice::from_raw_parts(HWCAPS_PATH.as_ptr(), 4) };

    // argv0 includes a terminator character. This comes in handy when interfacing with syscalls.
    let argv0 = extract_argv0(argv);

    let mut loader_path = make_uninit_array!(sys::MAX_PATH_LEN as usize);
    // Note: The linux kernel doesn't write a null terminator. Since loader_path is an uninitialized array,
    //       we cannot assume there's a null terminator.

    let loader_end_index = get_loader_path(&mut loader_path);

    let bin_index = BIN_PATH.len();
    let usr_index = USR_PATH.len();

    //Make sure we're not trying to execute ourselves!
    #[cfg(feature = "self_execution_check")]
    unsafe {
        let cmp1 = argv0.get_unchecked(..argv0.len()-1); //Ignore terminator
        let cmp2 = loader_path.get_unchecked(bin_index..loader_end_index);
        if cmp1.ends_with(cmp2) {
            abort!(exit_code::SELF_EXECUTION, "Recursion error! Do not run the loader directly!")
        }
    };

    let mut cwd = sys::AT_FDCWD;

    // When argv0 is a command alias (foo -> /usr/bin/foo, for example)
    // Set cwd to our binary's parent (normally /usr/bin)
    let path_kind = get_path_kind(&argv0);

    if path_kind == -1 {
        //Sneakily put a null byte here without making a new string
        let byte = loader_path[bin_index];
        loader_path[bin_index] = b'\0';

        let c_str = unsafe { CStr::from_bytes_with_nul_unchecked(&loader_path) };

        cwd = match sys::openat(sys::AT_FDCWD, c_str, sys::O_PATH) {
            Ok(d) => d,
            Err(e) => abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path Resolution: IO error while determining cwd! ({})", e.into_raw())
        };
        //Restore the previous character
        loader_path[bin_index] = byte;
    }

    let mut cmd_path = make_uninit_array!(sys::MAX_PATH_LEN as usize);
    let cmd_path_len = resolve_path(cwd, argv0, &mut cmd_path);

    // cmd_path_len+1 must fit in cmd_path, because of the terminator.
    if cmd_path_len+1 >= cmd_path.len() {
        abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target: Path too large!")
    }

    // These aren't problematic because argv0 is guaranteed to be  bytes long
    let cmd_path_usr_slice = unsafe { cmd_path.get_unchecked(..usr_index) };
    let cmd_path_bin_slice = unsafe { cmd_path.get_unchecked(usr_index..cmd_path_len+1) };

    // Check if our target's on /usr/
    if cmd_path_usr_slice != USR_PATH {
        abort!(exit_code::TARGET_PATH_INVALID, "Target: Invalid location!")
    }

    // Prepare execution target path
    let base_length = HWCAPS_PATH.len();
    // Very hacky and unsafe code :)
    // We can reuse the string we already have instead of allocating a new one, saving on time.
    let mut target_path = loader_path;

    // We've already determined the path starts with /usr/, so we only need to copy from hwcaps/
    // Copy the part of the path which we won't be changing anymore
    let copy_index = unsafe {
        let src = HWCAPS_PATH.get_unchecked(usr_index..);
        let copy_index = usr_index+src.len();
        let dst = target_path.get_unchecked_mut(usr_index..usr_index+src.len());
        dst.copy_from_slice(src);
        copy_index
    };

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
            let mut target_relative_slice = unsafe {
                target_path.get_unchecked_mut(copy_index..)
            };

            let (relative_char_index, arch_name_len) = match capabilities::format_arch_name(&mut target_relative_slice, i) {
                Ok(v) => v,
                Err(_) => abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target: Path too large!"),
            };
            version_char_index = relative_char_index + copy_index;

            // Copy the relative bin path
            // cmd_path_bin_slice already includes the null character!
            path_len = base_length + arch_name_len + cmd_path_bin_slice.len() - 1;

            if path_len > sys::MAX_PATH_LEN as usize {
                abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target: Path too large!")
            }

            unsafe {
                let copy_index = copy_index + arch_name_len;
                let src = cmd_path_bin_slice;
                let dst = target_path.get_unchecked_mut(copy_index..copy_index + cmd_path_bin_slice.len());
                dst.copy_from_slice(src);
            }

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
                    let slice = slice::from_raw_parts(target_path.as_ptr(), path_len);
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
