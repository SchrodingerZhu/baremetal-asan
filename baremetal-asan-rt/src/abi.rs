//! Compiler/runtime ABI compatibility symbols.

/// Link-time marker for ASan ABI version 8; no runtime work is required.
#[unsafe(no_mangle)]
pub extern "C" fn __asan_version_mismatch_check_v8() {}
