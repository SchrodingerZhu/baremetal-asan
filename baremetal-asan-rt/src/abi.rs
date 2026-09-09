//! Compiler/runtime ABI compatibility symbols.

/// Export the link-time marker for ASan ABI version 8.
#[macro_export]
macro_rules! export_asan_abi {
    () => {
        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_version_mismatch_check_v8() {}
    };
}
