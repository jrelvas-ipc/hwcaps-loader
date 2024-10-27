use crate::sys;
use crate::logging;

// NON-LIBC LINKING//

#[cfg(target_os="none")]
#[cfg_attr(target_arch = "x86", path = "entry_point_x86.rs")]
#[cfg_attr(target_arch = "x86_64", path = "entry_point_x86.rs")]
mod entry_point;

// LIBC LINKING //

#[cfg(not(target_os="none"))]
#[link(name = "c")]
extern "C" {
}

//TODO: use when https://doc.rust-lang.org/unstable-book/language-features/lang-items.html stabilizes
//#[lang = "eh_personality"]
//extern "C" fn eh_personality() {}

//Workarounds for https://github.com/rust-lang/rust/issues/106864
#[no_mangle]
extern "C" fn rust_eh_personality() {}

// PANIC //

#[cfg(debug_assertions)]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;

    let message = _info.message();
    let location = _info.location().unwrap();

    let mut buffer = [0; 1024];
    let mut writer = logging::PrintBuff::new(&mut buffer);

    let _ = write!(&mut writer, "Error: {message}\nAt: {location}\n");

    _ = sys::write(sys::STDOUT, &buffer);
    sys::exit(logging::ErrorCode::RustPanic as u8)
}

#[cfg(not(debug_assertions))]
// We can't do panic on production...
// core::fmt increases binary size by an obscene amount
// and we can't use tfmt because PanicInfo is too tied to core::fmt
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    sys::exit(logging::ErrorCode::RustPanic as u8)
}
