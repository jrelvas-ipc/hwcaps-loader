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
mod path;
mod output;

use core::ffi::{c_char, CStr};
use core::slice;

use sys::ExitCode;
use output::abort;

const HWCAPS_PATH: &'static [u8] = b"/usr/hwcaps/";
const USR_PATH: &'static [u8] = b"/usr";
const BIN_PATH: &'static [u8] = b"/usr/bin/";

fn extract_argv0(ptr: *const *const c_char) -> &'static [u8]  {
    let argv0 = unsafe {
        let ptr = *ptr; // Modern linux kernels guarantee argv0's existence, so no need to check if the pointer is null

        // from_ptr uses strlen() internally, provided by libc or as a compiler langitem
        CStr::from_ptr(ptr).to_bytes_with_nul()
    };

    if argv0.len() > sys::MAX_PATH_LEN as usize || argv0.len() < 1 {
        abort(ExitCode::CommandPathInvalid, "Command path doesn't fit bounds!", 0, None)
    }

    argv0
}

fn get_loader_path(buffer: &mut [u8]) -> usize {
    let loader_size = match sys::readlink(c"/proc/self/exe", buffer) {
        Ok(p) => p,
        Err(e) => abort(ExitCode::ProcPathIOError, "Failed to read loader path!", e.into_raw() as u32, None)
    };

    // It's safe to do this because the buffers passed to this function are always 4096 bytes
    if unsafe { buffer.get_unchecked(1..BIN_PATH.len())  != BIN_PATH.get_unchecked(1..) } {
        abort(ExitCode::ProcPathInvalid, "Invalid loader binary location!", 0, None)
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
            abort(ExitCode::PathResolutionIOError, "Failed to resolve path!", e.into_raw() as u32, Some(path))
        }
    };

    // Four digits should be enough for our purposes
    let mut fd_path = *b"/dev/fd/\0\0\0\0\0";
    path::itoa(fd as u32, &mut fd_path[8..]);

    let fd_cstr = unsafe { CStr::from_bytes_with_nul_unchecked(&fd_path) };

    match sys::readlink(fd_cstr, buffer) {
        Ok(p) => p,
        Err(e) => abort(ExitCode::PathResolutionIOError, "Failed to resolve path!", e.into_raw() as u32, Some(&fd_path))
    }
}


#[no_mangle]
pub extern fn main(_argc: i32, argv: *const *const c_char, envp: *const *const c_char) -> ! {
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
            abort(ExitCode::SelfExecution, "Do not run hwcaps-loader directly!", 0, None)
        }
    };

    let mut cwd = sys::AT_FDCWD;

    // When argv0 is a command alias (foo -> /usr/bin/foo, for example)
    // Set cwd to our binary's parent (normally /usr/bin)
    let path_kind = path::get_kind(&argv0);

    if path_kind == -1 {
        //Sneakily put a null byte here without making a new string
        let byte = loader_path[bin_index];
        loader_path[bin_index] = b'\0';

        let c_str = unsafe { CStr::from_bytes_with_nul_unchecked(&loader_path) };

        cwd = match sys::openat(sys::AT_FDCWD, c_str, sys::O_PATH) {
            Ok(d) => d,
            Err(e) => abort(ExitCode::PathResolutionIOError, "Failed to get parent directory of loader!", e.into_raw() as u32, None)
        };
        //Restore the previous character
        loader_path[bin_index] = byte;
    }

    let mut cmd_path = make_uninit_array!(sys::MAX_PATH_LEN as usize);
    let cmd_path_len = resolve_path(cwd, argv0, &mut cmd_path);

    // cmd_path_len+1 must fit in cmd_path, because of the terminator.
    if cmd_path_len+1 >= cmd_path.len() {
        abort(ExitCode::TargetPathTooLarge, "Target path too large!", 0, None)
    }

    // These aren't problematic because argv0 is guaranteed to be  bytes long
    let cmd_path_usr_slice = unsafe { cmd_path.get_unchecked(..usr_index) };
    let cmd_path_bin_slice = unsafe { cmd_path.get_unchecked(usr_index..cmd_path_len+1) };

    // Check if our target's on /usr/
    if cmd_path_usr_slice != USR_PATH {
        abort(ExitCode::TargetPathInvalid, "Invalid target location!", 0, None)
    }

    // Prepare execution target path
    let base_length = HWCAPS_PATH.len() + cmd_path_bin_slice.len();

    // Very hacky and unsafe code :)iov_base
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

    // Determine the maximum feature level supported by this machine
    let feature_level = capabilities::get_max_feature_level();

    // Generate a path for every available feature level, then attempt to execute it.
    // Repeat until execve() is sucessful or we run out of levels.
    for i in (0..=feature_level).rev() {
        let mut path_len = 0;

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
                Err(_) => abort(ExitCode::TargetPathTooLarge, "Target path too large!", 0, None)
            };
            version_char_index = relative_char_index + copy_index;

            // Copy the relative bin path
            path_len = base_length + arch_name_len;

            if path_len > sys::MAX_PATH_LEN as usize {
                abort(ExitCode::TargetPathTooLarge, "Target path too large!", path_len as u32, None)
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
            let path_buffer = unsafe { slice::from_raw_parts(target_path.as_ptr(), path_len) };
            output::debug_print("(DEBUG) Executing target.", 0, Some(path_buffer));
        }

        let str_ptr = target_path.as_ptr() as *const i8;
        let c_str = unsafe { CStr::from_ptr(str_ptr) };

        match sys::execve(c_str, argv, envp).into_raw() {
            sys::ENOENT => continue,
            other => {
                let path_buffer = unsafe { slice::from_raw_parts(target_path.as_ptr(), path_len) };
                abort(ExitCode::TargetExecutionError, "Failed to execute target binary!", other as u32, Some(path_buffer))
            }
        };
    }

    abort(ExitCode::TargetNoViableBinaries, "Program has no supported binaries available. Is it installed properly?", 0, None)
}
