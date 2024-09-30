#![no_std]
#![no_main]
use syscalls::{syscall, Sysno};

#[cfg(not(target_os="none"))]
compile_error!("empty_binary must be built as a standalone target!");

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    unsafe {
        _ = syscall!(Sysno::exit, 0);
        core::hint::unreachable_unchecked()
    }
}
