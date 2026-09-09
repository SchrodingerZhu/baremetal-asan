#![no_std]
//! Portable ASan runtime building blocks. Target crates choose a layout and
//! instantiate the C ABI with [`export_asan!`]. This library owns no panic handler.

mod abi;
pub mod access;
mod diagnostic;
mod global;
pub mod layout;
pub mod memory;
pub mod stack;

#[cfg(test)]
mod test_layout;

/// Export the currently implemented ASan C ABI for one layout.
///
/// Invoke once in the target runtime crate. `stack_top` is a function returning
/// the exclusive top of the current downward-growing stack, or zero to skip
/// no-return cleanup. The target owns shadow initialization and the panic handler.
/// Individual `export_asan_*` macros can instead export selected ABI groups.
#[macro_export]
macro_rules! export_asan {
    ($layout:ty, stack_top = $stack_top:path $(,)?) => {
        $crate::export_asan_abi!();
        $crate::export_asan_globals!();
        $crate::export_asan_access!($layout);
        $crate::export_asan_memory!($layout);
        $crate::export_asan_stack!($layout, stack_top = $stack_top);
    };
}
