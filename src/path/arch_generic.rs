use core::cmp;

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
