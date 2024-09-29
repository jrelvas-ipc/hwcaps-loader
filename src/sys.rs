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

use core::ffi::c_int;
//use core::ffi::c_size_t;
//use core::ffi::c_ssize_t;
use core::ffi::c_char;
use core::ffi::CStr;
use core::cell;

use syscalls::{Sysno, syscall, Errno};

//TODO: remove this when https://github.com/rust-lang/rust/issues/88345 is stabilized
#[allow(non_camel_case_types)]
type c_size_t = usize;
#[allow(non_camel_case_types)]
type c_ssize_t = isize;

pub const MAX_ARG_LEN: c_size_t = 131072;
pub const MAX_PATH_LEN: c_ssize_t = 4096;

pub const STDOUT: c_int = 1;
pub const AT_FDCWD: c_int = -100;

pub const O_NOFOLLOW: c_int = 0o400000;
pub const O_PATH: c_int = 0o10000000;
pub const O_CLOEXEC: c_int = 0x80000;

pub const ENOENT: c_int = 2;

#[link(name = "c")]
extern "C" {
}

#[inline]
pub fn exit(code: i32) -> ! {
    unsafe {
        _ = syscall!(Sysno::exit, code);
        core::hint::unreachable_unchecked()
    }
}

#[inline]
pub fn write(fd: i32, buffer: &[u8]) -> Result<usize, Errno> {
    unsafe { syscall!(Sysno::write, fd, buffer.as_ptr(), buffer.len()) }
}

#[macro_export] macro_rules! write_message {
    ($arg:expr) => {
        _ = sys::write(sys::STDOUT, $arg);
    };
}

#[inline]
pub fn readlink(path: &CStr, buffer: &mut [u8]) -> Result<usize, Errno> {
    unsafe { syscall!(Sysno::readlink, path.as_ptr(), buffer.as_mut_ptr(), buffer.len()) }
}

#[inline]
pub fn openat(dirfd: i32, path: &CStr, flags: c_int) -> Result<i32, Errno> {
    let result = unsafe { syscall!(Sysno::openat, dirfd, path.as_ptr(), O_CLOEXEC | flags) };
    match result {
        Ok(fd) => {
            return Ok(fd as i32)
        },
        Err(e) => return Err(e),
    }
}

#[inline]
pub fn execve(path: &CStr, argv: *const *const c_char, envp: *const *const c_char) -> Errno {
     unsafe {
        let result = syscall!(Sysno::execve, path.as_ptr(), argv, envp);
        //Execve doesn't return, so it's safe to assume an error occured
        result.err().unwrap_unchecked()
    }
}
