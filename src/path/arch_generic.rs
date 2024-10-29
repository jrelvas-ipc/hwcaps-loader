use core::{cmp, hint};
use crate::BIN_PATH;

// Returns -1 if path is alias
// Returns 0 if path starts with "/" (absolute)
// Returns 1 if path starts with "./" (relative)
// Returns 2 if path starts with "../" (relative)
#[allow(dead_code)]
pub fn get_kind(path: &[u8]) -> i32 {
    let last = cmp::min(path.len()-1, 2);

    for i in 0..last {
        let byte = unsafe { *path.get_unchecked(i) };

        if byte == b'/' { return i as i32 };
        if byte != b'.' { break };
    }
    -1
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

pub fn is_loader_binary(loader_path: &[u8], argv0_path: &[u8]) -> bool {
    if loader_path.len() <= BIN_PATH.len() {return false};
    let loader_name = &loader_path[BIN_PATH.len()..];

    if argv0_path.len() - 1 <= loader_name.len() {return false};
    let argv0_name = &argv0_path[argv0_path.len()-1-loader_name.len()..argv0_path.len()-1];

    // We use this simple (but unoptimized) loop here due to Rust using very large (600+ bytes)
    // intrisic functions for array/slice comparisons which don't fit in a architectural word (1/2/4/8) bytes.
    // If libc is linked, its implementation of memcmp is used instead, bypassing this issue.
    #[cfg(target_os="none")]
    {
        for i in 0..argv0_name.len()-1 {
            if loader_name[i] == argv0_name[i] {
                return true
            }
        }
        false
    }

    #[cfg(not(target_os="none"))]
    return loader_name == argv0_name;
}
