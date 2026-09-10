//! Checked memory operations emitted in place of compiler memory intrinsics.

use core::ffi::{c_int, c_void};
use core::ptr;

use crate::diagnostic::report_memcpy_overlap;
use crate::{access::check_range, platform::Platform};

/// Check both ranges, copy non-overlapping bytes, and return `dst`.
/// Identical source and destination are accepted, as in the host ASan runtime.
///
/// # Safety
/// Shadow RAM must satisfy `Platform::to_slices` throughout the check.
/// For nonzero `size`, `src` must be readable and `dst` writable for `size` bytes.
pub unsafe fn memcpy<P: Platform>(
    dst: *mut c_void,
    src: *const c_void,
    size: usize,
) -> *mut c_void {
    if size == 0 {
        return dst;
    }
    unsafe { check_range::<P>(src as usize, size, false) };
    unsafe { check_range::<P>(dst as usize, size, true) };
    if !ptr::eq(dst, src) {
        if (dst as usize).abs_diff(src as usize) < size {
            report_memcpy_overlap(dst as usize, src as usize, size);
        }
        // SAFETY: the caller supplies valid ranges and overlap was ruled out.
        unsafe { ptr::copy_nonoverlapping(src.cast::<u8>(), dst.cast::<u8>(), size) };
    }
    dst
}

/// Check both ranges, copy bytes with overlap support, and return `dst`.
///
/// # Safety
/// Shadow RAM must satisfy `Platform::to_slices` throughout the check.
/// For nonzero `size`, `src` must be readable and `dst` writable for `size` bytes.
pub unsafe fn memmove<P: Platform>(
    dst: *mut c_void,
    src: *const c_void,
    size: usize,
) -> *mut c_void {
    if size != 0 {
        unsafe { check_range::<P>(src as usize, size, false) };
        unsafe { check_range::<P>(dst as usize, size, true) };
        // SAFETY: the caller supplies valid ranges; copy permits overlap.
        unsafe { ptr::copy(src.cast::<u8>(), dst.cast::<u8>(), size) };
    }
    dst
}

/// Check the destination, fill it with the low byte of `value`, and return it.
///
/// # Safety
/// Shadow RAM must satisfy `Platform::to_slices` throughout the check.
/// For nonzero `size`, `dst` must be writable for `size` bytes.
pub unsafe fn memset<P: Platform>(dst: *mut c_void, value: c_int, size: usize) -> *mut c_void {
    if size != 0 {
        unsafe { check_range::<P>(dst as usize, size, true) };
        // SAFETY: the caller supplies a writable range; bytes have alignment 1.
        unsafe { ptr::write_bytes(dst.cast::<u8>(), value as u8, size) };
    }
    dst
}

/// Export checked memcpy, memmove, and memset for the supplied platform.
#[macro_export]
macro_rules! export_asan_memory {
    ($platform:ty) => {
        /// # Safety
        /// Source, destination, and shadow must satisfy the runtime's memcpy contract.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_memcpy(
            dst: *mut ::core::ffi::c_void,
            src: *const ::core::ffi::c_void,
            size: usize,
        ) -> *mut ::core::ffi::c_void {
            unsafe { $crate::memory::memcpy::<$platform>(dst, src, size) }
        }

        /// # Safety
        /// Source, destination, and shadow must satisfy the runtime's memmove contract.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_memmove(
            dst: *mut ::core::ffi::c_void,
            src: *const ::core::ffi::c_void,
            size: usize,
        ) -> *mut ::core::ffi::c_void {
            unsafe { $crate::memory::memmove::<$platform>(dst, src, size) }
        }

        /// # Safety
        /// Destination and shadow must satisfy the runtime's memset contract.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_memset(
            dst: *mut ::core::ffi::c_void,
            value: ::core::ffi::c_int,
            size: usize,
        ) -> *mut ::core::ffi::c_void {
            unsafe { $crate::memory::memset::<$platform>(dst, value, size) }
        }
    };
}

#[cfg(test)]
mod tests;
