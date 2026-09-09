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

#[cold]
#[inline(never)]
pub(crate) fn report_memcpy_overlap(dst: usize, src: usize, size: usize) -> ! {
    panic!("ASan: memcpy of {size} byte(s) has overlapping ranges at {dst:#x} and {src:#x}");
}
