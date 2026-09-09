//! No-op global hooks while global sanitization is disabled.
//!
//! These stubs leave descriptors, registration flags, and shadow memory untouched.

use core::ffi::{c_char, c_void};

#[unsafe(no_mangle)]
pub extern "C" fn __asan_register_globals(_globals: *mut c_void, _count: usize) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_unregister_globals(_globals: *mut c_void, _count: usize) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_register_elf_globals(
    _flag: *mut usize,
    _start: *mut c_void,
    _stop: *mut c_void,
) {
}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_unregister_elf_globals(
    _flag: *mut usize,
    _start: *mut c_void,
    _stop: *mut c_void,
) {
}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_register_image_globals(_flag: *mut usize) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_unregister_image_globals(_flag: *mut usize) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_before_dynamic_init(_module_name: *const c_char) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_after_dynamic_init() {}
