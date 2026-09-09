//! ASan runtime diagnostics.

#[cold]
#[inline(never)]
pub(crate) fn report_access(addr: usize, size: usize, is_write: bool, invalid: usize) -> ! {
    panic!(
        "ASan: {} of {} byte(s) at {:#x}; first invalid address {:#x}",
        if is_write { "store" } else { "load" },
        size,
        addr,
        invalid,
    );
}
