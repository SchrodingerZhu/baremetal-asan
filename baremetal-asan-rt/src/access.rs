//! Outlined checks for application memory described by a platform.

use crate::{diagnostic::report_access, platform::Platform};
use scan::ShadowScan;

mod scan;

/// `SIZE` is the fixed ABI access size, or zero for a runtime-sized access.
#[inline(always)]
unsafe fn check_access<P: Platform, const SIZE: usize>(addr: usize, size: usize, is_write: bool) {
    if size != 0 && addr.checked_add(size - 1).is_none() {
        // SAFETY: the caller supplies initialized, stable shadow.
        unsafe { report_access::<P>(addr, size, is_write, addr) };
    }
    // SAFETY: RAM shadow is initialized at startup; ROM shadow is precomputed.
    // Shadow remains unchanged while borrowed; each slice stays in one region.
    let invalid = unsafe { P::to_shadow_slices(addr, size) }.find_map(|part| {
        let first = part.memory.start;
        let last = part.memory.end - 1;
        if SIZE != 0 && SIZE <= 2 * P::GRANULE {
            // Fixed ABI accesses cover at most three shadow bytes. Borrow arrays
            // so their scan remains straight-line even under -Os, including when
            // an access is split between two backing regions.
            if SIZE == 1 || part.bytes.len() == 1 {
                part.bytes
                    .first_chunk::<1>()
                    .unwrap()
                    .find_invalid_shadow_byte::<P>(first, last)
            } else if SIZE <= P::GRANULE || part.bytes.len() == 2 {
                part.bytes
                    .first_chunk::<2>()
                    .unwrap()
                    .find_invalid_shadow_byte::<P>(first, last)
            } else {
                part.bytes
                    .first_chunk::<3>()
                    .unwrap()
                    .find_invalid_shadow_byte::<P>(first, last)
            }
        } else {
            part.bytes.find_invalid_shadow_byte::<P>(first, last)
        }
    });
    if let Some(invalid) = invalid {
        // SAFETY: the caller supplies initialized, stable shadow.
        unsafe { report_access::<P>(addr, size, is_write, invalid) };
    }
}

/// Check a fixed-size access using the selected platform.
///
/// # Safety
/// Shadow RAM must be initialized and readable, without concurrent mutation
/// during this check, as required by `Platform::to_shadow_slices`.
#[inline(always)]
pub unsafe fn check_access_fixed<P: Platform, const SIZE: usize>(addr: usize, is_write: bool) {
    // SAFETY: the caller supplies the platform's initialized shadow RAM.
    unsafe { check_access::<P, SIZE>(addr, SIZE, is_write) };
}

/// Check a runtime-sized range for either the ABI or other runtime modules.
///
/// # Safety
/// The same shadow-access requirements as `check_access_fixed` apply.
#[inline]
pub unsafe fn check_range<P: Platform>(addr: usize, size: usize, is_write: bool) {
    // SAFETY: the caller supplies the platform's initialized shadow RAM.
    unsafe { check_access::<P, 0>(addr, size, is_write) };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __asan_access_checkers {
    ($platform:ty; $($load:ident, $store:ident, $size:literal;)+) => {
        $(
            /// # Safety
            /// The target must provide initialized shadow and prevent conflicting access.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $load(addr: usize) {
                unsafe { $crate::access::check_access_fixed::<$platform, $size>(addr, false) };
            }

            /// # Safety
            /// The target must provide initialized shadow and prevent conflicting access.
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $store(addr: usize) {
                unsafe { $crate::access::check_access_fixed::<$platform, $size>(addr, true) };
            }
        )+
    };
}

/// Export outlined load/store checks for the supplied platform.
#[macro_export]
macro_rules! export_asan_access {
    ($platform:ty) => {
        $crate::__asan_access_checkers! {
            $platform;
            __asan_load1, __asan_store1, 1;
            __asan_load2, __asan_store2, 2;
            __asan_load4, __asan_store4, 4;
            __asan_load8, __asan_store8, 8;
            __asan_load16, __asan_store16, 16;
        }

        /// # Safety
        /// The target must provide initialized shadow and prevent conflicting access.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_loadN(addr: usize, size: usize) {
            unsafe { $crate::access::check_range::<$platform>(addr, size, false) };
        }

        /// # Safety
        /// The target must provide initialized shadow and prevent conflicting access.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_storeN(addr: usize, size: usize) {
            unsafe { $crate::access::check_range::<$platform>(addr, size, true) };
        }
    };
}

#[cfg(test)]
mod tests;
