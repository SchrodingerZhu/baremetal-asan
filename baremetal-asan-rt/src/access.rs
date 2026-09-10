//! Outlined checks for application memory described by a layout.

use crate::{diagnostic::report_access, layout::Layout};
use scan::ShadowScan;

mod scan;

/// `SIZE` is the fixed ABI access size, or zero for a runtime-sized access.
#[inline(always)]
unsafe fn check_access<L: Layout, const SIZE: usize>(addr: usize, size: usize, is_write: bool) {
    if size != 0 && addr.checked_add(size - 1).is_none() {
        // SAFETY: the caller supplies initialized, stable shadow RAM.
        unsafe { report_access::<L>(addr, size, is_write, addr) };
    }
    // SAFETY: startup initializes the layout's shadow RAM. Shadow must remain
    // unchanged while the check borrows it; each slice stays in one RAM region.
    let invalid = unsafe { L::to_slices(addr, size) }.find_map(|part| {
        let first = part.memory.start;
        let last = part.memory.end - 1;
        if SIZE != 0 && SIZE <= 2 * L::GRANULE {
            // Fixed ABI accesses cover at most three shadow bytes. Borrow arrays
            // so their scan remains straight-line even under -Os, including when
            // an access is split between two backing regions.
            if SIZE == 1 || part.bytes.len() == 1 {
                part.bytes
                    .first_chunk::<1>()
                    .unwrap()
                    .find_invalid_shadow_byte::<L>(first, last)
            } else if SIZE <= L::GRANULE || part.bytes.len() == 2 {
                part.bytes
                    .first_chunk::<2>()
                    .unwrap()
                    .find_invalid_shadow_byte::<L>(first, last)
            } else {
                part.bytes
                    .first_chunk::<3>()
                    .unwrap()
                    .find_invalid_shadow_byte::<L>(first, last)
            }
        } else {
            part.bytes.find_invalid_shadow_byte::<L>(first, last)
        }
    });
    if let Some(invalid) = invalid {
        // SAFETY: the caller supplies initialized, stable shadow RAM.
        unsafe { report_access::<L>(addr, size, is_write, invalid) };
    }
}

/// Check a fixed-size access using the selected layout.
///
/// # Safety
/// Shadow RAM must be initialized and readable, without concurrent mutation
/// during this check, as required by `Layout::to_slices`.
#[inline(always)]
pub unsafe fn check_access_fixed<L: Layout, const SIZE: usize>(addr: usize, is_write: bool) {
    // SAFETY: the caller supplies the layout's initialized shadow RAM.
    unsafe { check_access::<L, SIZE>(addr, SIZE, is_write) };
}

/// Check a runtime-sized range for either the ABI or other runtime modules.
///
/// # Safety
/// The same shadow-access requirements as `check_access_fixed` apply.
#[inline]
pub unsafe fn check_range<L: Layout>(addr: usize, size: usize, is_write: bool) {
    // SAFETY: the caller supplies the layout's initialized shadow RAM.
    unsafe { check_access::<L, 0>(addr, size, is_write) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __asan_access_checkers {
    ($layout:ty; $($load:ident, $store:ident, $size:literal;)+) => {
        $(
            /// # Safety
            /// The target must initialize shadow RAM and prevent conflicting access.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $load(addr: usize) {
                unsafe { $crate::access::check_access_fixed::<$layout, $size>(addr, false) };
            }

            /// # Safety
            /// The target must initialize shadow RAM and prevent conflicting access.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $store(addr: usize) {
                unsafe { $crate::access::check_access_fixed::<$layout, $size>(addr, true) };
            }
        )+
    };
}

/// Export outlined load/store checks for the supplied layout.
#[macro_export]
macro_rules! export_asan_access {
    ($layout:ty) => {
        $crate::__asan_access_checkers! {
            $layout;
            __asan_load1, __asan_store1, 1;
            __asan_load2, __asan_store2, 2;
            __asan_load4, __asan_store4, 4;
            __asan_load8, __asan_store8, 8;
            __asan_load16, __asan_store16, 16;
        }

        /// # Safety
        /// The target must initialize shadow RAM and prevent conflicting access.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_loadN(addr: usize, size: usize) {
            unsafe { $crate::access::check_range::<$layout>(addr, size, false) };
        }

        /// # Safety
        /// The target must initialize shadow RAM and prevent conflicting access.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_storeN(addr: usize, size: usize) {
            unsafe { $crate::access::check_range::<$layout>(addr, size, true) };
        }
    };
}

#[cfg(test)]
mod tests;
