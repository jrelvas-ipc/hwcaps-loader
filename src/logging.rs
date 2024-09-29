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

macro_rules! write_str {
    ($($arg:tt)*) => {
        fn write_str(&mut self, s: &str) -> Result<(), $($arg)*> {
            let bytes = s.as_bytes();

            unsafe {
                // Skip over already-copied data
                let remainder = self.buf.get_unchecked_mut(self.offset..);
                // Check if there is space remaining (return error instead of panicking)
                if remainder.len() < bytes.len() { return Err($($arg)*); }
                // Make the two slices the same length
                let remainder = remainder.get_unchecked_mut(..bytes.len());
                // Copy
                remainder.copy_from_slice(bytes);

                // Update offset to avoid overwriting
                self.offset += bytes.len();
            }

            Ok(())
        }
    };
}


impl<'a> tfmt::uWrite for PrintBuff<'a> {
    type Error = ();

    write_str!(());
}

impl<'a> core::fmt::Write for PrintBuff<'a> {
    write_str!(core::fmt::Error);
}

#[macro_export] macro_rules! abort {
    ($exit_code:expr) => {{
        print!(b"An error has occured!\n");
        sys::exit($exit_code)
    }};
    ($exit_code:expr, $($arg:tt)*) => {{
        print!($($arg)*);
        sys::exit($exit_code)
    }}
}

#[macro_export] macro_rules! print {
    ($($arg:tt)*) => {{
        let mut buffer = [0; 1024];
        let mut writer = PrintBuff::new(&mut buffer);
        _ = tfmt::uwriteln!(&mut writer, $($arg)*);

        write_message!(&mut buffer);
    }}
}
