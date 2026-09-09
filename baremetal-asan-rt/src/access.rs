//! Outlined access checks for application SRAM; other address ranges are ignored.

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
    Some(addr_to_shadow_unchecked(addr))
}

/// Map an address already known to be within application SRAM.
#[inline(always)]
fn addr_to_shadow_unchecked(addr: usize) -> usize {
    RAM_START + (addr - RAM_START) / ASAN_QUANTUM
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

/// Fixed sizes touch at most three granules, so spell them out instead of looping.
#[inline(always)]
fn first_poisoned_fixed<const SIZE: usize>(
    addr: usize,
    mut read_shadow: impl FnMut(usize) -> i8,
) -> Option<usize> {
    if addr < RAM_START + RAM_OFFSET || addr > RAM_START + RAM_SIZE - SIZE {
        // Preserve clipping and overflow behavior for accesses at RAM boundaries.
        return first_poisoned(addr, SIZE, read_shadow);
    }

    let shadow_addr = addr_to_shadow_unchecked(addr);
    let granule = addr - addr % ASAN_QUANTUM;
    let last = addr + SIZE - 1;
    let first = read_shadow(shadow_addr);
    if first != 0 {
        if let Some(invalid) = poisoned_in_granule(first, granule, addr, last) {
            return Some(invalid);
        }
    }
    if SIZE > 1 && last >= granule + ASAN_QUANTUM {
        let second = read_shadow(shadow_addr + 1);
        if second != 0 {
            let start = granule + ASAN_QUANTUM;
            if let Some(invalid) = poisoned_in_granule(second, start, start, last) {
                return Some(invalid);
            }
        }
    }
    if SIZE > ASAN_QUANTUM && last >= granule + 2 * ASAN_QUANTUM {
        let third = read_shadow(shadow_addr + 2);
        if third != 0 {
            let start = granule + 2 * ASAN_QUANTUM;
            return poisoned_in_granule(third, start, start, last);
        }
    }
    None
}

#[inline(always)]
fn poisoned_in_granule(shadow: i8, granule: usize, first: usize, last: usize) -> Option<usize> {
    if shadow < 0 {
        Some(first)
    } else if last.min(granule + ASAN_QUANTUM - 1) >= granule + shadow as usize {
        Some(first.max(granule + shadow as usize))
    } else {
        None
    }
}

#[inline(always)]
fn check_fixed<const SIZE: usize>(addr: usize, is_write: bool) {
    if addr < RAM_START + RAM_OFFSET || addr > RAM_START + RAM_SIZE - SIZE {
        // Tail-call the general checker instead of keeping arguments live across
        // its return. The in-range path needs no general scanning loop.
        check_access(addr, SIZE, is_write);
        return;
    }
    let invalid = first_poisoned_fixed::<SIZE>(addr, |shadow| {
        // SAFETY: only initialized, reserved shadow RAM is read.
        unsafe { (shadow as *const i8).read_volatile() }
    });
    if let Some(invalid) = invalid {
        report_access(addr, SIZE, is_write, invalid);
    }
}

macro_rules! access_checkers {
    ($($load:ident, $store:ident, $size:literal;)+) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $load(addr: usize) {
                check_fixed::<$size>(addr, false);
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn $store(addr: usize) {
                check_fixed::<$size>(addr, true);
            }
        )+
    };
}

access_checkers! {
    __asan_load1, __asan_store1, 1;
    __asan_load2, __asan_store2, 2;
    __asan_load4, __asan_store4, 4;
    __asan_load8, __asan_store8, 8;
    __asan_load16, __asan_store16, 16;
}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_loadN(addr: usize, size: usize) {
    check_access(addr, size, false);
}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_storeN(addr: usize, size: usize) {
    check_access(addr, size, true);
}

#[cfg(test)]
mod tests;
