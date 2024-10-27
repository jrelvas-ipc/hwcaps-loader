use crate::sys::{writev, IOVector, exit, STDOUT};
use core::mem::MaybeUninit;

#[derive(Debug, PartialEq, Eq)]
pub enum ErrorCode {
    Ok = 0,
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

pub struct PrintBuff<'a> {
    buf: &'a mut [u8],
    offset: usize,
}

impl<'a> PrintBuff<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        PrintBuff {
            buf,
            offset: 0,
        }
    }
}

use core::fmt;
impl<'a> fmt::Write for PrintBuff<'a> {
    fn write_str(&mut self, s: &str) -> Result<(), fmt::Error> {
        let bytes = s.as_bytes();

        unsafe {
            // Skip over already-copied data
            let remainder = self.buf.get_unchecked_mut(self.offset..);
            // Check if there is space remaining (return error instead of panicking)
            if remainder.len() < bytes.len() { return Err(fmt::Error); }
            // Make the two slices the same length
            let remainder = remainder.get_unchecked_mut(..bytes.len());
            // Copy
            remainder.copy_from_slice(bytes);

            // Update offset to avoid overwriting
            self.offset += bytes.len();
        }
        Ok(())
    }
}

pub fn itoa(mut n: u32, arr: &mut [u8]) -> usize {
    let mut last_digit = false;
    let mut i = 0;

    while !last_digit {
        if n < 10 {last_digit = true};

        let digit = (n % 10) as u8;
        arr[i] = digit + b'0';
        n /= 10;
        i += 1;
    }

    arr[..i].reverse();
    i
}

#[inline(always)]
fn print(msg: &'static str, errno: u32, path: Option<&[u8]>) {
    let mut array: [MaybeUninit<IOVector>; 9] = [const { MaybeUninit::uninit() }; 9];
    let mut offset = 0;

    let mut write_part = |buf: &[u8]| {
        array[offset].write(IOVector::new(buf));
        offset += 1;
    };

    write_part(b"hwcaps-loader: ");
    write_part(&msg.as_bytes());

    let mut errno_buffer: [u8; 16];
    if errno != 0 {
        write_part(b" | Errno: ");

        errno_buffer = [0; 16];
        let len = itoa(errno, &mut errno_buffer);

        write_part(&errno_buffer[..len]);
    }
    match path {
        Some(p) => {
            write_part(b" | Path: ");
            write_part(p);
        },
        _ => ()
    }

    write_part(b"\n");

    writev(STDOUT, (array).as_ptr(), offset);
}

#[cold]
pub fn debug_print(msg: &'static str, errno: u32, path: Option<&[u8]>) {
    print(msg, errno, path);
}

#[cold]
pub fn abort(err: ErrorCode, msg: &'static str, errno: u32, path: Option<&[u8]>) -> ! {
    print(msg, errno, path);
    exit(err as u8)
}
