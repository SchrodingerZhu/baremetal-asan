//! Stubs for compiler-emitted stack ABI calls.
//!
//! Fake-stack allocation returns zero to select the real stack. Other hooks
//! leave shadow memory untouched. Direct compiler-emitted shadow writes remain.

/// Disable runtime-selectable stack-use-after-return detection by default.
#[unsafe(no_mangle)]
pub static mut __asan_option_detect_stack_use_after_return: core::ffi::c_int = 0;

macro_rules! fake_stack_stubs {
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

fake_stack_stubs! {
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

macro_rules! shadow_stubs {
    ($($name:ident),+ $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub extern "C" fn $name(_addr: usize, _size: usize) {}
        )+
    };
}

shadow_stubs! {
    __asan_set_shadow_00,
    __asan_set_shadow_01,
    __asan_set_shadow_02,
    __asan_set_shadow_03,
    __asan_set_shadow_04,
    __asan_set_shadow_05,
    __asan_set_shadow_06,
    __asan_set_shadow_07,
    __asan_set_shadow_f1,
    __asan_set_shadow_f2,
    __asan_set_shadow_f3,
    __asan_set_shadow_f5,
    __asan_set_shadow_f8,
}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_poison_stack_memory(_addr: usize, _size: usize) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_unpoison_stack_memory(_addr: usize, _size: usize) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_alloca_poison(_addr: usize, _size: usize) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_allocas_unpoison(_top: usize, _bottom: usize) {}

#[unsafe(no_mangle)]
pub extern "C" fn __asan_handle_no_return() {}
