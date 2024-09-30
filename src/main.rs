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

use core::str;
use core::ffi::c_char;
use core::ffi::CStr;
use core::fmt::Write;
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
    def!(COMMAND_PATH_INVALID_ANCESTOR, -70211);
    def!(PROC_PATH_IO_ERROR, -70220);
    def!(PROC_PATH_EMPTY, -70221);
    def!(PROC_PATH_NO_PARENT, -70222);
    def!(PROC_PATH_NO_GRANDPARENT, -70223);
    def!(PATH_RESOLUTION_IO_ERROR, -70230);
    def!(TARGET_PATH_TOO_LARGE, -70240);
    def!(TARGET_EXECUTION_ERROR, -70241);
    def!(TARGET_NO_VIABLE_BINARIES, -70242);
}

//TODO: use when https://doc.rust-lang.org/unstable-book/language-features/lang-items.html stabilizes
//#[lang = "eh_personality"]
//extern "C" fn eh_personality() {}

//Workarounds for https://github.com/rust-lang/rust/issues/106864
#[no_mangle]
extern "C" fn rust_eh_personality() {}

#[cfg(debug_assertions)]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    let message = _info.message();
    let location = _info.location().unwrap();

    let mut buffer = [0; 1024];
    let mut writer = PrintBuff::new(&mut buffer);

    let _ = write!(&mut writer, "Error: {message}\nAt: {location}\n");

    write_message!(&buffer);
    sys::exit(exit_code::RUST_PANIC)
}

#[cfg(not(debug_assertions))]
// We can't do panic on production...
// core::fmt increases binary size by an obscene amount
// and we can't use tfmt because PanicInfo is too tied to core::fmt
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    sys::exit(exit_code::RUST_PANIC)
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

fn get_exec_path(buffer: &mut [u8]) -> (usize, usize, usize) {
    let exec_size = match sys::readlink(c"/proc/self/exe", buffer) {
        Ok(p) => p,
        Err(e) => abort!(exit_code::PROC_PATH_IO_ERROR, "Loader path: IO Error! ({})", e.into_raw())
    };

    if !(exec_size > 0 ){
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

    (exec_size, last_dash, second_last_dash)
}

fn resolve_path(cwd_fd: i32, path: &str, buffer: &mut [u8]) -> usize {
    let str_ptr = path.as_ptr() as *const i8;
    let c_str = unsafe { CStr::from_ptr(str_ptr) };

    let fd = match sys::openat(cwd_fd, c_str, sys::O_PATH | sys::O_NOFOLLOW) {
        Ok(d) => d,
        Err(e) => abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path Resolution error! Failed to open \"{}\"! (errno: {})", &path, e.into_raw())
    };

    // There are three default FDs on Linux: 0 (STDOUT); 1 (STDIN); 2 (STDERR)
    // Since we only ever open a single file descriptor in hwcaps-loader, it's usual for the FD to be 3...
    // ...unless the program which executed us didn't close its FDs...
    // use a fast path for fd 3, while including a formatting fallback for other FDs.
    let mut fd_path;
    let c_str = if fd == 3 {
        c"/dev/fd/3"
    } else {
        fd_path = [0; 1024];
        let mut writer = PrintBuff::new(&mut fd_path);

        _ = tfmt::uwrite!(&mut writer, "/dev/fd/{}\0", fd);
        unsafe { CStr::from_bytes_with_nul_unchecked(&fd_path) }
    };

    match sys::readlink(c_str, buffer) {
        Ok(p) => p,
        //Err(e) => abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path Resolution error! Failed to get path of FD \"{}\"! (errno: {})", fd, e.into_raw())
        Err(e) => abort!(exit_code::PATH_RESOLUTION_IO_ERROR, "Path Resolution error! Failed to get path of FD \"{}\"! (errno: {})", fd, e.into_raw())
    }
}

#[cfg(target_os="none")]
#[naked]
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    // Jump to `entry`, passing it the initial stack pointer value as an
    // argument, a null return address, a null frame pointer, and an aligned
    // stack pointer. On many architectures, the incoming frame pointer is
    // already null.
    core::arch::asm!(
        "mov rdi, rsp", // Pass the incoming `rsp` as the arg to `entry`.
        "push rbp",     // Set the return address to zero.
        "jmp {entry}",  // Jump to `entry`.
        entry = sym _main_proxy,
        options(noreturn),
    )
}

pub unsafe extern "C" fn _main_proxy(mem: *const usize) -> ! {
    let kernel_argc = *mem;
    let argc = kernel_argc as i32;

    let argv = mem.add(1).cast::<*const c_char>();
    let envp = argv.add(argc as c_char as usize + 1);

    main(argc as i32, argv, envp)
}
#[no_mangle]
pub extern fn main(_argc: i32, argv: *const *const c_char, envp: *const *const c_char) -> ! {
    // We cheat here - argv0 and exec_path have a null terminator
    // (makes it easier to interface with C code without useless copies)
    // Modern linux kernels guarantee argv0's existence, so no need to check if the pointer is null
    let argv0 = get_arg_string(unsafe { *argv });

    let mut exec_path = [0; sys::MAX_PATH_LEN as usize];
    let (exec_len, bin_index, usr_index) = get_exec_path(&mut exec_path);

    //Make sure we're not trying to execute ourselves!
    #[cfg(feature = "self_execution_check")]
    if argv0.as_bytes().ends_with(&exec_path[bin_index+1..exec_len+1]){
        abort!(exit_code::SELF_EXECUTION, "Cannot execute own binary!")
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
            Err(e) => abort!(exit_code::COMMAND_PATH_INVALID_ANCESTOR, "Failed to open parent! (errno: {})", e.into_raw())
        };
        //Restore the previous character
        exec_path[bin_index + 1] = byte;
    }

    let mut buffer = [0; sys::MAX_PATH_LEN as usize];
    let argv0_len = resolve_path(cwd, argv0, &mut buffer);
    let argv0 = buffer;

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
        abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target path is too large!")
    }

    // Very hacky and unsafe code :)
    // We can reuse the string we already have instead of allocating a new one, saving on time.
    let mut target_path = exec_path;

    // We've already determined the path starts with this, so we can just skip over that
    let start = usr_index+1;
    let end = start+hwcaps_dir.len();
    target_path[start..end].copy_from_slice(hwcaps_dir);
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
            let (relative_char_index, arch_name_len) = match capabilities::format_arch_name(&mut target_path[end..], i) {
                Ok(v) => v,
                Err(_) => abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target path is too large!"),
            };
            version_char_index = relative_char_index + end;

            path_len = base_length + arch_name_len + second_half.len() + 1;
            if path_len > sys::MAX_PATH_LEN as usize {
                abort!(exit_code::TARGET_PATH_TOO_LARGE, "Target path is too large!")
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
            print!("(DEBUG) Executing: {}\n", path_string);
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
                abort!(exit_code::TARGET_EXECUTION_ERROR, "Failed to execute binary at \"{}\"! (errno: {})", path_string, other)

                //TODO: Use this when https://github.com/rust-lang/rust/issues/119206 is stabilized
                //abort!("Failed to execute program \"{}\"! (errno: {})", unsafe {str::from_raw_parts(target_path.as_ptr(), end)}, e)
            }
        };
    }

    abort!(exit_code::TARGET_NO_VIABLE_BINARIES, "Target program has no viable binaries! Is it installed properly?")
}
