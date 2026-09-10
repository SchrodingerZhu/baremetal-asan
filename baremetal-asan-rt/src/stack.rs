//! Generic stack cleanup and macros for compiler-emitted stack ABI calls.

use crate::platform::Platform;
use core::ptr;

/// Clear stack shadow through `Platform::stack_top()`; zero disables cleanup.
///
/// # Safety
/// The current stack grows downward toward this function's frame. Its shadow
/// must be initialized and exclusively accessible during cleanup.
#[inline(never)]
pub unsafe fn handle_no_return<P: Platform>() {
    let top = P::stack_top();
    if top == 0 {
        return;
    }

    // A local in this frame lies below the caller's frame on a downward-growing
    // stack. Keep this hook outlined so the marker belongs to our own frame.
    let marker = 0u8;
    let bottom = ptr::addr_of!(marker) as usize;
    // The linker symbol denotes the stack's exclusive upper bound.
    if bottom >= top {
        return;
    }

    // SAFETY: cleanup has exclusive access to the current stack's initialized
    // shadow. The platform clips the range and splits it into backing RAM regions.
    unsafe { P::to_shadow_slices_mut(bottom, top - bottom) }.for_each(|part| part.bytes.fill(0));
}

#[doc(hidden)]
#[macro_export]
macro_rules! __asan_fake_stack_stubs {
    ($($malloc:ident, $malloc_always:ident, $free:ident;)+) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $malloc(_size: usize) -> usize {
                0
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn $malloc_always(_size: usize) -> usize {
                0
            }

            #[unsafe(no_mangle)]
            pub extern "C" fn $free(_addr: usize, _size: usize) {}
        )+
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __asan_shadow_setters {
    ($platform:ty; $($name:ident => $value:literal),+ $(,)?) => {
        $(
            /// Fill logical shadow bytes; ignore empty or invalid ranges.
            ///
            /// # Safety
            /// Shadow RAM must be initialized and exclusively accessible.
            #[unsafe(no_mangle)]
            #[inline(never)]
            pub unsafe extern "C" fn $name(addr: usize, size: usize) {
                // SAFETY: startup initializes and reserves the platform's shadow
                // RAM; shadow updates require exclusive access to these bytes.
                unsafe { <$platform as $crate::platform::Platform>::set_shadow(addr, size, $value as u8 as i8) };
            }
        )+
    };
}

/// Export stack setters, no-return cleanup, and the current fake-stack stubs.
#[macro_export]
macro_rules! export_asan_stack {
    ($platform:ty $(,)?) => {
        /// Runtime-selectable stack-use-after-return detection is disabled.
        #[unsafe(no_mangle)]
        pub static mut __asan_option_detect_stack_use_after_return: ::core::ffi::c_int = 0;

        $crate::__asan_fake_stack_stubs! {
            __asan_stack_malloc_0, __asan_stack_malloc_always_0, __asan_stack_free_0;
            __asan_stack_malloc_1, __asan_stack_malloc_always_1, __asan_stack_free_1;
            __asan_stack_malloc_2, __asan_stack_malloc_always_2, __asan_stack_free_2;
            __asan_stack_malloc_3, __asan_stack_malloc_always_3, __asan_stack_free_3;
            __asan_stack_malloc_4, __asan_stack_malloc_always_4, __asan_stack_free_4;
            __asan_stack_malloc_5, __asan_stack_malloc_always_5, __asan_stack_free_5;
            __asan_stack_malloc_6, __asan_stack_malloc_always_6, __asan_stack_free_6;
            __asan_stack_malloc_7, __asan_stack_malloc_always_7, __asan_stack_free_7;
            __asan_stack_malloc_8, __asan_stack_malloc_always_8, __asan_stack_free_8;
            __asan_stack_malloc_9, __asan_stack_malloc_always_9, __asan_stack_free_9;
            __asan_stack_malloc_10, __asan_stack_malloc_always_10, __asan_stack_free_10;
        }

        $crate::__asan_shadow_setters! {
            $platform;
            __asan_set_shadow_00 => 0x00,
            __asan_set_shadow_01 => 0x01,
            __asan_set_shadow_02 => 0x02,
            __asan_set_shadow_03 => 0x03,
            __asan_set_shadow_04 => 0x04,
            __asan_set_shadow_05 => 0x05,
            __asan_set_shadow_06 => 0x06,
            __asan_set_shadow_07 => 0x07,
            __asan_set_shadow_f1 => 0xf1,
            __asan_set_shadow_f2 => 0xf2,
            __asan_set_shadow_f3 => 0xf3,
            __asan_set_shadow_f5 => 0xf5,
            __asan_set_shadow_f8 => 0xf8,
        }

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_poison_stack_memory(_addr: usize, _size: usize) {}

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_unpoison_stack_memory(_addr: usize, _size: usize) {}

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_alloca_poison(_addr: usize, _size: usize) {}

        #[unsafe(no_mangle)]
        pub extern "C" fn __asan_allocas_unpoison(_top: usize, _bottom: usize) {}

        /// # Safety
        /// The current stack's initialized shadow must be exclusively accessible.
        #[unsafe(no_mangle)]
        #[inline(never)]
        pub unsafe extern "C" fn __asan_handle_no_return() {
            unsafe { $crate::stack::handle_no_return::<$platform>() };
        }
    };
}
