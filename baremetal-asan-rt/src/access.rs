//! Outlined access checks for application SRAM; other address ranges are ignored.

use crate::diagnostic::report_access;
use scan::ShadowScan;

mod scan;

// Assume RA8x2 without security mode for now, 2MiB total SRAM
// Lower 0x04_0000 bytes are allocated for shadow
pub const RAM_START: usize = 0x2000_0000;
pub const RAM_OFFSET: usize = 0x0004_0000;
pub const RAM_SIZE: usize = 0x0020_0000;
pub const ASAN_QUANTUM: usize = 1 << 3;

/// Only application SRAM is sanitized by this prototype.
pub fn check_in_range(addr: usize) -> bool {
    (RAM_START + RAM_OFFSET..RAM_START + RAM_SIZE).contains(&addr)
}

/// Map an SRAM address to one shadow byte per `ASAN_QUANTUM` bytes.
pub fn addr_to_shadow(addr: usize) -> Option<usize> {
    if !check_in_range(addr) {
        return None;
    }
    Some(addr_to_shadow_unchecked(addr))
}

/// Return the shadow bytes covering the application-SRAM portion of an access.
/// Includes partially covered granules; empty, wrapping, or non-overlapping
/// ranges return `None`.
///
/// Shadow RAM must be initialized and remain unchanged while the slice is in use.
#[inline(always)]
pub fn slice_to_shadow_slice(addr: usize, size: usize) -> Option<&'static [i8]> {
    let last = addr
        .checked_add(size.checked_sub(1)?)?
        .min(RAM_START + RAM_SIZE - 1);
    let first = addr.max(RAM_START + RAM_OFFSET);
    if first > last {
        return None;
    }

    let shadow_start = addr_to_shadow_unchecked(first);
    let shadow_len = addr_to_shadow_unchecked(last) - shadow_start + 1;
    // SAFETY: The clipped range maps entirely into reserved shadow RAM, and
    // its length fits in isize. The runtime must keep this RAM initialized
    // and unmodified while the returned slice is borrowed.
    Some(unsafe { core::slice::from_raw_parts(shadow_start as *const i8, shadow_len) })
}

/// Borrow the shadow of a fixed-size access as an array without copying.
/// Returns `None` unless the mapped shadow has exactly `SHADOW_SIZE` bytes.
/// The same range and shadow-RAM requirements as `slice_to_shadow_slice` apply.
#[inline(always)]
pub fn slice_to_shadow_slice_fixed<const SIZE: usize, const SHADOW_SIZE: usize>(
    addr: usize,
) -> Option<&'static [i8; SHADOW_SIZE]> {
    slice_to_shadow_slice(addr, SIZE)?.try_into().ok()
}

/// Map an address already known to be within application SRAM.
#[inline(always)]
fn addr_to_shadow_unchecked(addr: usize) -> usize {
    RAM_START + (addr - RAM_START) / ASAN_QUANTUM
}

/// Find the first poisoned address in the validated SRAM range `first..=last`.
/// `shadow` contains exactly the granules covering that range.
#[inline(always)]
fn first_poisoned<Shadow: ShadowScan>(first: usize, last: usize, shadow: Shadow) -> Option<usize> {
    let base = first - first % ASAN_QUANTUM;
    shadow.find_map(|index, value| {
        let granule = base + index * ASAN_QUANTUM;
        poisoned_in_granule(value, granule, first.max(granule), last)
    })
}

#[inline(always)]
fn check_access<Shadow: ShadowScan>(
    addr: usize,
    size: usize,
    is_write: bool,
    shadow: Option<Shadow>,
) {
    let Some(shadow) = shadow else {
        // A wrapping range is invalid; empty and non-SRAM accesses are ignored.
        if size != 0 && addr.checked_add(size - 1).is_none() {
            report_access(addr, size, is_write, addr);
        }
        return;
    };
    // Slice construction already validated the range and ruled out overflow.
    let first = addr.max(RAM_START + RAM_OFFSET);
    let last = (addr + (size - 1)).min(RAM_START + RAM_SIZE - 1);
    if let Some(invalid) = first_poisoned(first, last, shadow) {
        report_access(addr, size, is_write, invalid);
    }
}

/// Select the exact array length for a 1/2/4/8/16-byte access, including
/// unaligned accesses and accesses clipped at the SRAM boundaries.
#[inline(always)]
fn check_access_fixed<const SIZE: usize>(addr: usize, is_write: bool) {
    let one = slice_to_shadow_slice_fixed::<SIZE, 1>(addr);
    if SIZE == 1 || one.is_some() {
        check_access(addr, SIZE, is_write, one);
        return;
    }
    let two = slice_to_shadow_slice_fixed::<SIZE, 2>(addr);
    if SIZE <= ASAN_QUANTUM || two.is_some() {
        check_access(addr, SIZE, is_write, two);
        return;
    }
    check_access(
        addr,
        SIZE,
        is_write,
        slice_to_shadow_slice_fixed::<SIZE, 3>(addr),
    );
}

/// Check `first..=last` against one shadow byte, returning the first invalid
/// application address or `None`.
///
/// `granule` is the aligned start of an 8-byte application-memory block.
/// `first` must lie inside that block; `last` is inclusive and may extend
/// beyond it, so the check clips `last` to the block's end.
///
/// Shadow 0 allows the whole block; a negative value poisons it entirely.
/// A positive value allows that many leading bytes of the block.
#[inline(always)]
fn poisoned_in_granule(shadow: i8, granule: usize, first: usize, last: usize) -> Option<usize> {
    if shadow == 0 {
        None
    } else if shadow < 0 {
        Some(first)
    } else if last.min(granule + ASAN_QUANTUM - 1) >= granule + shadow as usize {
        Some(first.max(granule + shadow as usize))
    } else {
        None
    }
}

macro_rules! access_checkers {
    ($($load:ident, $store:ident, $size:literal;)+) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $load(addr: usize) {
                check_access_fixed::<$size>(addr, false);
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn $store(addr: usize) {
                check_access_fixed::<$size>(addr, true);
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
    check_access(addr, size, false, slice_to_shadow_slice(addr, size));
}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_storeN(addr: usize, size: usize) {
    check_access(addr, size, true, slice_to_shadow_slice(addr, size));
}

#[cfg(test)]
mod tests;
