use crate::sys;
use crate::exit_code;

// NON-LIBC LINKING//

#[cfg(target_os="none")]
#[cfg_attr(target_arch = "x86", path = "arch_x86.rs")]
#[cfg_attr(target_arch = "x86_64", path = "arch_x86.rs")]
mod arch;

#[cfg(target_os="none")]
pub unsafe extern "C" fn _main_proxy(mem: *const usize) -> ! {
    use core::ffi::c_char;

    let kernel_argc = *mem;
    let argc = kernel_argc as i32;

    let argv = mem.add(1).cast::<*const c_char>();
    let envp = argv.add(argc as c_char as usize + 1);

    super::main(argc as i32, argv, envp)
}

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
    use crate::PrintBuff;
    use crate::write_message;

    let message = _info.message();
    let location = _info.location().unwrap();

    let mut buffer = [0; 1024];
    let mut writer = PrintBuff::new(&mut buffer);

    let _ = write!(&mut writer, "Error: {message}\nAt: {location}\n");

    write_message!(&buffer);
    sys::exit(exit_code::RUST_PANIC)
}

#[cfg(not(debug_assertions))]
// We can't do panic on production...
// core::fmt increases binary size by an obscene amount
// and we can't use tfmt because PanicInfo is too tied to core::fmt
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    sys::exit(exit_code::RUST_PANIC)
}
