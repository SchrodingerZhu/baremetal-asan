//! Checked memory operations emitted in place of compiler memory intrinsics.

use core::ffi::{c_int, c_void};
use core::ptr;

use crate::access::check_range;
use crate::diagnostic::report_memcpy_overlap;

/// Check both ranges, copy non-overlapping bytes, and return `dst`.
/// Identical source and destination are accepted, as in the host ASan runtime.
///
/// # Safety
/// For nonzero `size`, `src` must be readable and `dst` writable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __asan_memcpy(
    dst: *mut c_void,
    src: *const c_void,
    size: usize,
) -> *mut c_void {
    if size == 0 {
        return dst;
    }
    check_range(src as usize, size, false);
    check_range(dst as usize, size, true);
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
/// For nonzero `size`, `src` must be readable and `dst` writable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __asan_memmove(
    dst: *mut c_void,
    src: *const c_void,
    size: usize,
) -> *mut c_void {
    if size != 0 {
        check_range(src as usize, size, false);
        check_range(dst as usize, size, true);
        // SAFETY: the caller supplies valid ranges; copy permits overlap.
        unsafe { ptr::copy(src.cast::<u8>(), dst.cast::<u8>(), size) };
    }
    dst
}

/// Check the destination, fill it with the low byte of `value`, and return it.
///
/// # Safety
/// For nonzero `size`, `dst` must be writable for `size` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __asan_memset(dst: *mut c_void, value: c_int, size: usize) -> *mut c_void {
    if size != 0 {
        check_range(dst as usize, size, true);
        // SAFETY: the caller supplies a writable range; bytes have alignment 1.
        unsafe { ptr::write_bytes(dst.cast::<u8>(), value as u8, size) };
    }
    dst
}

#[cfg(test)]
mod tests;
