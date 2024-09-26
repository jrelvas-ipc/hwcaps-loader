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

mod sys;
mod capabilities;

use core::str;
use core::ffi::c_char;
use core::ffi::CStr;
use core::fmt::Write;
use core::cell;
use core::slice;

use memchr::{memchr, memrchr};
use arrayvec::ArrayString;
use itoa;

//TODO: use when https://doc.rust-lang.org/unstable-book/language-features/lang-items.html stabilizes
//#[lang = "eh_personality"]
//extern "C" fn eh_personality() {}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    let message = _info.message();
    let location = _info.location().unwrap();

    //Don't allocate to the heap...
    let mut string = ArrayString::<1024>::new();

    let _ = write!(&mut string, "Error: {message}\nAt: {location}");
    let _ = sys::write(sys::STDOUT, &string.as_bytes());

    sys::exit(1);
}

fn get_arg_string(ptr: *const c_char) -> &'static str {
    let arg_slice = unsafe { slice::from_raw_parts(ptr as *mut u8, sys::MAX_ARG_LEN as usize) };

    let terminator_index = memchr(b'\0', &arg_slice)
        .expect("No terminator in buffer!");

    return unsafe { str::from_utf8_unchecked(&arg_slice[..terminator_index+1])};

}

fn get_exec_path() -> (sys::MutStackSlice, usize, usize, usize) {
    let (exec_path, exec_size) = match sys::readlink(c"/proc/self/exe") {
        Ok(p) => p,
        Err(e) => panic!("Failed to read exec magic link! (errno: {})", e)
    };

    let exec_path = exec_path.into_inner();

    if !(exec_size > 0 ){
        panic!("Exec magic link leads to empty path!")
    }

    let last_dash = match memrchr(b'/', &exec_path) {
        Some(i) => i,
        _ => panic!("Exec magic link has no parent!")
    };

    let second_last_dash = match memrchr(b'/', &exec_path[..last_dash]) {
        Some(i) => i,
        _ => panic!("Exec magic link has no grandparent!")
    };

    (cell::UnsafeCell::new(exec_path), exec_size, last_dash, second_last_dash)
}

fn resolve_path(cwd_fd: i32, path: &str) -> (sys::MutStackSlice, usize) {
    let str_ptr = path.as_ptr() as *const i8;
    let c_str = unsafe { CStr::from_ptr(str_ptr) };

    let fd = match sys::openat(cwd_fd, c_str, sys::O_PATH | sys::O_NOFOLLOW) {
        Ok(d) => d,
        Err(e) => panic!("Failed to open \"{}\"! (errno: {})", &path, e)
    };

    static FD_TEMPLATE: &'static [u8] = b"/proc/self/fd/";

    let fd_path = &mut [0; sys::MAX_PATH_LEN as usize];
    fd_path[..FD_TEMPLATE.len()].clone_from_slice(FD_TEMPLATE);

    let mut number_buffer = itoa::Buffer::new();
    let fd_buffer = number_buffer.format(fd as u32);

    fd_path[FD_TEMPLATE.len()..FD_TEMPLATE.len()+fd_buffer.len()].clone_from_slice(fd_buffer.as_bytes());

    let c_str = unsafe { CStr::from_bytes_with_nul_unchecked(fd_path) };

    match sys::readlink(c_str) {
        Ok(p) => p,
        Err(e) => panic!("Failed to get path of FD \"{}\"! (errno: {})", fd, e)
    }
}


#[no_mangle]
pub extern fn main(_argc: i32, argv: *const *const c_char, envp: *const *const c_char) {
    // We cheat here - argv0 and exec_path have a null terminator
    // (makes it easier to interface with C code without useless copies)
    // Modern linux kernels guarantee argv0's existence, so no need to check if the pointer is null
    let argv0 = get_arg_string(unsafe { *argv });
    let (exec_path, exec_len, bin_index, usr_index) = get_exec_path();

    let mut exec_path = exec_path.into_inner();

    //Make sure we're not trying to execute ourselves!
    #[cfg(feature = "self_execution_check")]
    if argv0.as_bytes().ends_with(&exec_path[bin_index+1..exec_len+1]){
        panic!("Cannot execute own binary!")
    }

    let mut cwd = sys::AT_FDCWD;

    // When argv0 is a command alias (foo -> /usr/bin/foo, for example)
    // Set cwd to our binary's parent (normally /usr/bin)
    if !argv0.starts_with("/") && !argv0.starts_with("./") && !argv0.starts_with("../") {
        //Sneakily put a null byte here without making a new string
        let byte = exec_path[bin_index + 1];
        exec_path[bin_index + 1] = b'\0';

        let c_str = unsafe { CStr::from_bytes_with_nul_unchecked(&exec_path) };

        cwd = match sys::openat(sys::AT_FDCWD, c_str, sys::O_PATH) {
            Ok(d) => d,
            Err(e) => panic!("Failed to open parent! (errno: {})", e)
        };
        //Restore the previous character
        exec_path[bin_index + 1] = byte;
    }

    let (argv0, argv0_len) = resolve_path(cwd, argv0);
    let argv0 = argv0.into_inner();

    let first_half = &argv0[..usr_index+1];
    let second_half = &argv0[usr_index..argv0_len];

    // Check if our target's on a valid location
    if first_half != &exec_path[..usr_index + 1] {
        panic!("hwcaps-loader symlink does not belong to its grandparent!")
    }

    // Prepare execution target path
    // TODO: Determine capabilities/featureset of CPU and choose the featureset directory based on that.
    let hwcaps_dir = b"hwcaps/";

    let base_length = first_half.len() + hwcaps_dir.len();
    if base_length > sys::MAX_PATH_LEN as usize {
        panic!("Path is too large")
    }

    // Very hacky and unsafe code :)
    // We can reuse the string we already have instead of allocating a new one, saving on time.
    let mut target_path = exec_path;

    // We've already determined the path starts with this, so we can just skip over that
    let start = usr_index+1;
    let end = start+hwcaps_dir.len();
    target_path[start..end].clone_from_slice(hwcaps_dir);
    let start = end;

    let mut must_format_arch = true;
    let mut version_char_index: usize = 0;
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
            let (relative_char_index, arch_name_len) = capabilities::format_arch_name(&mut target_path[end..], i);
            version_char_index = relative_char_index + end;

            path_len = base_length + arch_name_len + second_half.len() + 1;
            if path_len > sys::MAX_PATH_LEN as usize {
                panic!("Path is too large")
            }

            let start = start + arch_name_len;
            let end = start + second_half.len();
            target_path[start..end].clone_from_slice(&second_half);
            target_path[end]= b'\0';

            must_format_arch = false;
        }

        // Unless the arch name changes, all we need to do is update the character representing the arch version.
        target_path[version_char_index] = capabilities::HWCAPS_CHARS[i as usize];

        #[cfg(feature = "debug_print")]
        {
            _ = sys::write(sys::STDOUT, b"(DEBUG) Executing:\n");
            _ = sys::write(sys::STDOUT, &target_path[..path_len]);
            _ = sys::write(sys::STDOUT, b"\n");
        }

        let str_ptr = target_path.as_ptr() as *const i8;
        let c_str = unsafe { CStr::from_ptr(str_ptr) };

        match sys::execve(c_str, argv, envp) {
            Some(e) => {
                if e != sys::ENOENT {
                    let path_string = unsafe {
                        let slice = slice::from_raw_parts(target_path.as_ptr(), end);
                        str::from_utf8_unchecked(slice)
                    };
                    panic!("Failed to execute program \"{}\"! (errno: {})", path_string, e)

                    //TODO: Use this when https://github.com/rust-lang/rust/issues/119206 is stabilized
                    //panic!("Failed to execute program \"{}\"! (errno: {})", unsafe {str::from_raw_parts(target_path.as_ptr(), end)}, e)
                }
            },
            None => {} // This never happens - our program doesn't return
        };
    }

    _ = sys::write(sys::STDOUT, b"No viable binary to execute found!\n");
}

