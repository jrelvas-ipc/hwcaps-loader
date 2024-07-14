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
#![feature(lang_items, c_size_t)]
#![feature(start)]
#![feature(alloc_error_handler)]
#![feature(never_type)]

mod mem_alloc;
mod sys;

extern crate alloc;

use core::str;
use core::ffi::c_char;
use core::ffi::CStr;
use core::fmt::Write;

use alloc::slice;
use alloc::string::String;
use alloc::vec::Vec;

use memchr::{memchr, memrchr};
use arrayvec::ArrayString;

#[lang = "eh_personality"]
extern "C" fn eh_personality() {}

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

fn get_exec_path() -> (String, usize, usize) {
    let exec_path = match sys::readlink(c"/proc/self/exe") {
        Ok(p) => p,
        Err(e) => panic!("Failed to read exec magic link! (errno: {})", e)
    };

    if !(exec_path.len() > 0 ){
        panic!("Exec magic link leads to empty path!")
    }

    let bytes = exec_path.as_bytes();

    let last_dash = match memrchr(b'/', bytes) {
        Some(i) => i,
        _ => panic!("Exec magic link has no parent!")
    };

    let second_last_dash = match memrchr(b'/', &bytes[..last_dash]) {
        Some(i) => i,
        _ => panic!("Exec magic link has no grandparent!")
    };

    (exec_path, last_dash, second_last_dash)
}

fn resolve_path(cwd_fd: i32, path: &str) -> String {
    let str_ptr = path.as_ptr() as *const i8;
    let c_str = unsafe { CStr::from_ptr(str_ptr) };

    let fd = match sys::openat(cwd_fd, c_str, sys::O_PATH | sys::O_NOFOLLOW) {
        Ok(d) => d,
        Err(e) => panic!("Failed to open \"{}\"! (errno: {})", &path, e)
    };

    let mut fd_proc_path = String::with_capacity(sys::MAX_PATH_LEN as usize);
    write!(fd_proc_path, "/proc/self/fd/{}\0", fd).expect("Failed to write to string!");

    let str_ptr = fd_proc_path.as_ptr() as *const i8;
    let c_str = unsafe { CStr::from_ptr(str_ptr) };

    match sys::readlink(c_str) {
        Ok(p) => p,
        Err(e) => panic!("Failed to get path of FD \"{}\"! (errno: {})", fd, e)
    }
}


#[no_mangle]
pub extern fn main(argc: i32, argv: *const *const c_char, envp: *const *const c_char) {
    let arguments = unsafe { slice::from_raw_parts(argv, argc as usize) };
    // We cheat here - argv0 and exec_path have a null terminator
    // (makes it easier to interface with C code without useless copies)
    let argv0 = get_arg_string(arguments[0]);
    let (exec_path, bin_index, usr_index) = get_exec_path();

    //Make sure we're not trying to execute ourselves!
    if argv0.ends_with(&exec_path[bin_index+1..]){
        panic!("Cannot execute own binary!")
    }

    let mut cwd = sys::AT_FDCWD;

    // When argv0 is a command alias (foo -> /usr/bin/foo, for example)
    // Set cwd to our binary's parent (normally /usr/bin)
    if !argv0.starts_with("/") && !argv0.starts_with("./") && !argv0.starts_with("../") {
        //Clone to add a null pointer
        let mut terminated_parent = Vec::with_capacity(bin_index+1);
        terminated_parent.extend_from_slice(&exec_path.as_bytes()[..bin_index]);
        terminated_parent.push(b'\0');

        let c_str = unsafe { CStr::from_bytes_with_nul_unchecked(&terminated_parent) };

        cwd = match sys::openat(sys::AT_FDCWD, c_str, sys::O_PATH) {
            Ok(d) => d,
            Err(e) => panic!("Failed to open parent! (errno: {})", e)
        };
    }

    let argv0 = resolve_path(cwd, argv0);

    let (first_half, second_half) = argv0.split_at(usr_index + 1);

    // Check if our target's on a valid location
    if first_half != &exec_path[..usr_index + 1] {
        panic!("hwcaps-loader symlink does not belong to its grandparent!")
    }

    // Prepare execution target path
    // TODO: Determine capabilities/featureset of CPU and choose the featureset directory based on that.
    let hwcaps_dir = "hwcaps";
    let target_feature_set = "/x86-64-v1/";

    let mut target_path = String::with_capacity(
       argv0.len() + hwcaps_dir.len() + target_feature_set.len()
    );
    target_path = target_path + first_half + hwcaps_dir + target_feature_set + second_half;

    _ = sys::write(sys::STDOUT, b"(DEBUG) Executing:\n");
    _ = sys::write(sys::STDOUT, &target_path.as_bytes());
    _ = sys::write(sys::STDOUT, b"\n");

    let str_ptr = target_path.as_ptr() as *const i8;
    let c_str = unsafe { CStr::from_ptr(str_ptr) };

    match sys::execve(c_str, argv, envp) {
        Some(e) => panic!("Failed to execute program \"{}\"! (errno: {})", target_path, e),
        None => {} // This never happens - our program doesn't return
    };
}
