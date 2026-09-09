//! Target-specific stack bounds for no-return cleanup.

#[cfg(any(target_arch = "arm", all(target_arch = "x86_64", target_os = "linux")))]
#[inline(always)]
pub(crate) fn stack_top() -> usize {
    let top: usize;
    // Resolve the weak symbol in assembly so Rust cannot assume its address is
    // nonnull. An undefined ELF weak symbol resolves to zero.
    unsafe {
        #[cfg(target_arch = "arm")]
        core::arch::asm!(
            ".weak __stack",
            "ldr {top}, =__stack",
            top = out(reg) top,
            options(nostack, readonly, preserves_flags),
        );
        #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
        core::arch::asm!(
            ".weak __stack",
            "mov {top}, qword ptr [rip + __stack@GOTPCREL]",
            top = out(reg) top,
            options(nostack, readonly, preserves_flags),
        );
    }
    top
}

// Other hosts can still compile and test the layout without an ELF stack symbol.
#[cfg(not(any(target_arch = "arm", all(target_arch = "x86_64", target_os = "linux"))))]
#[inline(always)]
pub(crate) fn stack_top() -> usize {
    0
}
