// Returns -1 if path is alias
// Returns 0 if path starts with "/" (absolute)
// Returns 1 if path starts with "./" (relative)
// Returns 2 if path starts with "../" (relative)
pub fn get_kind(path: &[u8]) -> i32 {
    const COMPARE_DWORD: u32 = ((b'/' as u32) << 16) | ((b'.' as u32) << 8) | b'.' as u32;
    let mut ret = 2;

    #[allow(unused_assignments)]
    unsafe {
        core::arch::asm!(
            "2:", // Loop begin
            "and eax, {mask}", //Mask off the most significant byte. We only care about the first three.

            "cmp eax, {cmp}", // If the path dword matches our comparison dword, stop the loop
            "je 2f",

            "shld eax, ecx, 8", // Move a "." byte into our path dword.
            "dec edx",
            "jns 2b", // If ret isn't negative, keep the loop going. Otherwise, stop it - the path is
            "2:", // Loop end

            in("eax") *(path.as_ptr() as *const u32),
            in("ecx") ((b'.' as u32) << 24) | (b'.' as u32) << 16,
            inout("edx") ret,
            cmp = const COMPARE_DWORD,
            mask = const 0x00FFFFFF,
            options(nostack),
        )
    }
    ret
}

#[allow(unused_imports)]
pub use super::arch_fallback::*;
