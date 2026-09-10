#![no_std]
//! Portable ASan runtime building blocks. Target crates choose a platform and
//! instantiate the C ABI with [`export_asan!`]. This library owns no panic handler.

mod abi;
pub mod access;
mod diagnostic;
mod global;
pub mod heap;
pub mod memory;
pub mod platform;
pub mod stack;

#[cfg(test)]
mod test_platform;

/// Export the currently implemented ASan C ABI for one platform.
///
/// Invoke once in the target runtime crate with a type implementing
/// [`platform::Platform`]. The target owns shadow initialization and the panic handler.
/// Individual `export_asan_*` macros can instead export selected ABI groups.
#[macro_export]
macro_rules! export_asan {
    ($platform:ty $(,)?) => {
        static __ASAN_HEAP: $crate::heap::Heap<$platform> = $crate::heap::Heap::new();
        $crate::export_asan_abi!();
        $crate::export_asan_globals!();
        $crate::export_asan_access!($platform);
        $crate::export_asan_memory!($platform);
        $crate::export_asan_heap!($platform, __ASAN_HEAP);
        $crate::export_asan_stack!($platform, __ASAN_HEAP);
    };
}
