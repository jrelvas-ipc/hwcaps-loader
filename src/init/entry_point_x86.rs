#[no_mangle]
#[naked]
pub unsafe extern "C" fn _start() -> ! {
    core::arch::naked_asm!(
        //Get argc
        "mov rdi, rsp",

        //Get envp
        "movsx rax, byte ptr [rdi]",
        "lea rdx, [rdi + rax*8]",
        "add rdx, 16",

        //Get argv
        "lea rsi, [rdi + 8]",

        //Start main
        "call {entry}",
        entry = sym super::super::main
    )
}
