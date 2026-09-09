//! Scalar loop baseline retained for the RA8xx benchmark.

use crate::diagnostic::report_access;

// Assume RA8x2 without security mode for now, 2MiB total SRAM
// Lower 0x04_0000 bytes are allocated for shadow
pub const RAM_START: usize = 0x2000_0000;
pub const RAM_OFFSET: usize = 0x0004_0000;
pub const RAM_SIZE: usize = 0x0020_0000;
pub const ASAN_QUANTUM: usize = 1 << 3;

/// check if the address should be sanitized. For current quick prototype, we only sanitize SRAM region
pub fn check_in_range(addr: usize) -> bool {
    if addr < RAM_START + RAM_OFFSET {
        return false;
    }
    if addr >= RAM_START + RAM_SIZE {
        return false;
    }
    return true;
}

/// Map an SRAM address to one shadow byte per `ASAN_QUANTUM` bytes.
pub fn addr_to_shadow(addr: usize) -> Option<usize> {
    if !check_in_range(addr) {
        return None;
    }
    Some(RAM_START + (addr - RAM_START) / ASAN_QUANTUM)
}

/// Find the first poisoned byte in the SRAM portion of an access.
/// The reader takes a shadow address and returns its signed ASan value.
fn first_poisoned(
    addr: usize,
    size: usize,
    mut read_shadow: impl FnMut(usize) -> i8,
) -> Option<usize> {
    if size == 0 {
        return None;
    }
    let Some(last) = addr.checked_add(size - 1) else {
        // A wrapping access range is invalid; do not wrap into unrelated shadow.
        return Some(addr);
    };
    let last = last.min(RAM_START + RAM_SIZE - 1);
    let mut current = addr.max(RAM_START + RAM_OFFSET);
    let mut shadow_addr = addr_to_shadow(current)?;

    while current <= last {
        let shadow = read_shadow(shadow_addr);
        let granule = current - current % ASAN_QUANTUM;
        let end = (granule + ASAN_QUANTUM).min(last + 1);

        // Zero means fully accessible; negative values poison the whole granule.
        // Positive values give the number of accessible bytes at its beginning.
        if shadow < 0 {
            return Some(current);
        }
        if shadow > 0 && end - granule > shadow as usize {
            return Some(current.max(granule + shadow as usize));
        }
        current = end;
        shadow_addr += 1;
    }
    None
}

fn check_access(addr: usize, size: usize, is_write: bool) {
    let invalid = first_poisoned(addr, size, |shadow| {
        // SAFETY: first_poisoned only supplies addresses in reserved shadow RAM,
        // which must be initialized before instrumented code runs.
        unsafe { (shadow as *const i8).read_volatile() }
    });
    if let Some(invalid) = invalid {
        report_access(addr, size, is_write, invalid);
    }
}

macro_rules! access_checkers {
    ($($load:ident, $store:ident, $size:literal;)+) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $load(addr: usize) {
                check_access(addr, $size, false);
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn $store(addr: usize) {
                check_access(addr, $size, true);
            }
        )+
    };
}

access_checkers! {
    baseline_load1, baseline_store1, 1;
    baseline_load2, baseline_store2, 2;
    baseline_load4, baseline_store4, 4;
    baseline_load8, baseline_store8, 8;
    baseline_load16, baseline_store16, 16;
}

#[unsafe(no_mangle)]
pub extern "C" fn baseline_loadN(addr: usize, size: usize) {
    check_access(addr, size, false);
}

#[unsafe(no_mangle)]
pub extern "C" fn baseline_storeN(addr: usize, size: usize) {
    check_access(addr, size, true);
}
