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

/*
   This module contains all of the nasty low-level OS and linking stuff.
   The rest of hwcaps-loader should be (somewhat) OS agnostic.
*/

#[allow(unused_imports)]
#[allow(dead_code)]
#[allow(non_camel_case_types)]
#[allow(non_upper_case_globals)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
pub use bindings::*;

use core::ffi::{c_int, c_uint, c_void, /*c_size_t, c_ssize_t,*/ c_char, CStr};
use syscalls::{Sysno, syscall, Errno};

//TODO: remove this when https://github.com/rust-lang/rust/issues/88345 is stabilized
#[allow(non_camel_case_types)]
type c_size_t  = usize;
#[allow(non_camel_case_types)]
type c_ssize_t = isize;

pub const STDOUT: c_int = 1;

/*
   LINKING
   To have a functional program, we must provide the following members to
   the compiler and the linker:
   - entry point (_start) or external libc
   - panic_handler
   - rust_eh_personality
*/

/* For targets with no OS/ABI, link a minimal entry point (_start) function.*/
#[cfg(target_os="none")]
#[cfg_attr(target_arch = "x86", path = "entry_point/arch_x86.rs")]
#[cfg_attr(target_arch = "x86_64", path = "entry_point/arch_x86.rs")]
mod entry_point;

/* For targets with an OS/ABI, link libc */
#[cfg(not(target_os="none"))]
#[link(name = "c")]
extern "C" {}

//TODO: use when https://doc.rust-lang.org/unstable-book/language-features/lang-items.html stabilizes
//#[lang = "eh_personality"]
//extern "C" fn eh_personality() {}

//Workarounds for https://github.com/rust-lang/rust/issues/106864
#[no_mangle]
extern "C" fn rust_eh_personality() {}

// Debug panic handler
#[cfg(debug_assertions)]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    use core::fmt;
    use fmt::Write;

    use crate::output::debug::PrintBuff;

    let message = _info.message();
    let location = _info.location().unwrap();

    let mut buffer = [0; 1024];
    let mut writer = PrintBuff::new(&mut buffer);

    let _ = write!(&mut writer, "Error: {message}\nAt: {location}\n");

    _ = write(STDOUT, &buffer);
    exit(ExitCode::RustPanic as u8)
}


// Production panic handler
/* We can't do panic on production...
   core::fmt increases binary size by an obscene amount
   Just exist with a special error code if that happens */
#[cfg(not(debug_assertions))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    exit(ExitCode::RustPanic as u8)
}

/*
   SYSCALLS
   This part of the module implements wrappers for talking
   directly with the kernel (rather than using libc)
*/

#[derive(Debug, PartialEq, Eq)]
pub enum ExitCode {
    RustPanic = 100,
    SelfExecution = 200,
    CommandPathInvalid = 210,
    ProcPathIOError = 220,
    ProcPathInvalid = 221,
    PathResolutionIOError = 230,
    TargetPathInvalid = 240,
    TargetPathTooLarge = 241,
    TargetExecutionError = 242,
    TargetNoViableBinaries = 243
}

impl iovec {
    pub fn new(buffer: &[u8]) -> Self {
        iovec {
            iov_base: buffer.as_ptr() as *mut c_void,
            iov_len: buffer.len() as c_size_t,
        }
    }
}

#[macro_export] macro_rules! make_uninit_array {
    ($size:expr) => {{
        use core::mem::{transmute, MaybeUninit};

        let uninit = [const { MaybeUninit::<u8>::uninit() }; $size];
        #[allow(unused_unsafe)]
        unsafe { transmute::<[MaybeUninit<u8>; $size as usize], [u8; $size as usize]>(uninit) }
    }}
}

#[inline]
pub fn exit(code: u8) -> ! {
    unsafe {
        _ = syscall!(Sysno::exit, code);
        core::hint::unreachable_unchecked()
    }
}

#[inline]
pub fn writev(fd: i32, iovec: *const core::mem::MaybeUninit<iovec>, iovcnt: usize) -> Result<usize, Errno> {
    unsafe { syscall!(Sysno::writev, fd, iovec, iovcnt) }
}

#[allow(unused)] // This is only used by the panic function when debug_assertions are enabled
#[inline]
pub fn write(fd: i32, buffer: &[u8]) -> Result<usize, Errno> {
    unsafe { syscall!(Sysno::write, fd, buffer.as_ptr(), buffer.len()) }
}

#[inline]
pub fn readlink(path: &CStr, buffer: &mut [u8]) -> Result<usize, Errno> {
    unsafe {
        let ret = syscall!(Sysno::readlink, path.as_ptr(), buffer.as_mut_ptr(), buffer.len());
        /* man "readlink(2)":
           readlink()  places the contents of the symbolic link pathname in the buffer buf, which has size bufsiz.  read‐
           link() does not append a terminating null byte to buf.  It will (silently) truncate the contents (to a  length
           of bufsiz characters), in case the buffer is too small to hold all of the contents.
        */
        core::hint::assert_unchecked(ret.unwrap_unchecked() <= buffer.len());
        ret
    }
}

#[inline]
pub fn openat(dirfd: i32, path: &CStr, flags: c_uint) -> Result<i32, Errno> {
    let result = unsafe { syscall!(Sysno::openat, dirfd, path.as_ptr(), O_CLOEXEC | flags) };
    match result {
        Ok(fd) =>  return Ok(fd as i32),
        Err(e) => return Err(e),
    }
}

#[inline]
pub fn execve(path: &CStr, argv: *const *const c_char, envp: *const *const c_char) -> Errno {
     unsafe {
        let result = syscall!(Sysno::execve, path.as_ptr(), argv, envp);
        //Execve doesn't return, so it's safe to assume an error occured
        result.unwrap_err_unchecked()
    }
}
