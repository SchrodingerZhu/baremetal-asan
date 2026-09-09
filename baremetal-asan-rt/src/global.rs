//! No-op global hooks while global sanitization is disabled.

/// Export global stubs which leave descriptors, flags, and shadow untouched.
#[macro_export]
macro_rules! export_asan_globals {
    () => {
        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_register_globals(
            _globals: *mut ::core::ffi::c_void,
            _count: usize,
        ) {
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_unregister_globals(
            _globals: *mut ::core::ffi::c_void,
            _count: usize,
        ) {
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_register_elf_globals(
            _flag: *mut usize,
            _start: *mut ::core::ffi::c_void,
            _stop: *mut ::core::ffi::c_void,
        ) {
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_unregister_elf_globals(
            _flag: *mut usize,
            _start: *mut ::core::ffi::c_void,
            _stop: *mut ::core::ffi::c_void,
        ) {
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_register_image_globals(_flag: *mut usize) {}

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_unregister_image_globals(_flag: *mut usize) {}

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_before_dynamic_init(_module_name: *const ::core::ffi::c_char) {}

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_after_dynamic_init() {}
    };
}
