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
    #[link_name = "write"]
    fn write_c(fd: c_int, buf: *const u8, count: c_size_t) -> c_ssize_t;
    #[link_name = "exit"]
    fn exit_c(status: c_int) -> !;
    #[link_name = "__errno_location"]
    fn errno_location_c() -> *mut c_int;
    #[link_name = "readlink"]
    fn readlink_c(pathname: *const c_char, buf: *const u8, size_t: c_size_t) -> c_ssize_t;
    #[link_name = "openat"]
    fn openat_c(dirfd: c_int, pathname: *const c_char, flags: c_int, ...) -> c_int;
    #[link_name = "execve"]
    fn execve_c(pathname: *const c_char, argv: *const *const c_char, envp: *const *const c_char) -> c_int;
}

#[inline]
pub fn exit(code: i32) -> ! {
    unsafe { exit_c(code) }
}

#[inline]
pub fn write(fd: i32, buffer: &[u8]) -> Result<usize, i32> {
    let size = unsafe { write_c(fd, buffer.as_ptr(), buffer.len()) };

    if size < 0 {
        return Err(unsafe {*errno_location_c()})
    }

    Ok(size as usize)
}

pub type MutStackSlice = cell::UnsafeCell<[u8; MAX_PATH_LEN as usize]>;

#[inline]
pub fn readlink(path: &CStr) -> Result<(MutStackSlice, usize), i32> {
    let buffer = [0; MAX_PATH_LEN as usize];

    let size = unsafe {readlink_c(path.as_ptr(), buffer.as_ptr(), buffer.len()) };

    if size < 0 {
        return Err(unsafe {*errno_location_c()})
    }

    Ok((cell::UnsafeCell::new(buffer), size as usize))
}

#[inline]
pub fn openat(dirfd: i32, path: &CStr, flags: c_int) -> Result<i32, i32> {
    let fd = unsafe { openat_c(dirfd, path.as_ptr(), O_CLOEXEC | flags) };

    if fd < 0 {
        return Err(unsafe {*errno_location_c()})
    }

    Ok(fd)
}

#[inline]
pub fn execve(path: &CStr, argv: *const *const c_char, envp: *const *const c_char) -> Option<i32> {
    let ret = unsafe { execve_c(path.as_ptr(), argv, envp) };
    if ret == -1 {
        return Some(unsafe { *errno_location_c() })
    }

    None
}
