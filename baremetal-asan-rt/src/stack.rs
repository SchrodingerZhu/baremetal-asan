//! Compiler-emitted stack ABI calls.
//!
//! No-return cleanup clears the current stack's shadow when `__stack` is defined.
//! Shadow-fill helpers accept only ranges wholly inside reserved shadow RAM.
//! Fake-stack allocation returns zero to select the real stack. Lifetime and
//! dynamic-allocation hooks remain stubs.

use core::ptr;

use crate::access::{RAM_OFFSET, RAM_SIZE, RAM_START, addr_to_shadow_unchecked};

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

macro_rules! shadow_setters {
    ($($name:ident => $value:literal),+ $(,)?) => {
        $(
            /// Fill already-mapped shadow bytes; ignore empty or invalid ranges.
            #[unsafe(no_mangle)]
            #[inline(never)]
            pub extern "C" fn $name(addr: usize, size: usize) {
                let shadow_end = RAM_START + RAM_OFFSET;
                // Validate the whole range without overflowing addr + size.
                if size == 0
                    || !(RAM_START..shadow_end).contains(&addr)
                    || size > shadow_end - addr
                {
                    return;
                }
                // SAFETY: startup reserves this writable shadow RAM. The range
                // is fully contained in it; use raw writes without borrowing it.
                unsafe { ptr::write_bytes(addr as *mut u8, $value, size) };
            }
        )+
    };
}

shadow_setters! {
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

#[unsafe(no_mangle)]
#[inline(never)]
pub extern "C" fn __asan_handle_no_return() {
    let top: usize;
    // Resolve the weak symbol in assembly so Rust cannot assume its address is
    // nonnull. An undefined ELF weak symbol resolves to zero.
    unsafe {
        #[cfg(target_arch = "arm")]
        core::arch::asm!(
            ".weak __stack",
            "ldr {top}, =__stack",
            top = out(reg) top,
            options(nostack, readonly, preserves_flags),
        );
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            ".weak __stack",
            "mov {top}, qword ptr [rip + __stack@GOTPCREL]",
            top = out(reg) top,
            options(nostack, readonly, preserves_flags),
        );
    }
    if top == 0 {
        return;
    }

    // A local in this frame lies below the caller's frame on a downward-growing
    // stack. Keep this hook outlined so the marker belongs to our own frame.
    let marker = 0u8;
    let bottom = (ptr::addr_of!(marker) as usize).max(RAM_START + RAM_OFFSET);
    // The linker symbol denotes the stack's exclusive upper bound.
    let top = top.min(RAM_START + RAM_SIZE);
    if bottom >= top {
        return;
    }

    let shadow_start = addr_to_shadow_unchecked(bottom);
    let shadow_size = addr_to_shadow_unchecked(top - 1) - shadow_start + 1;
    // SAFETY: the clipped range maps into reserved shadow RAM. Use raw writes:
    // cleanup mutates shadow and must not construct a shared shadow slice.
    unsafe { ptr::write_bytes(shadow_start as *mut u8, 0, shadow_size) };
}
