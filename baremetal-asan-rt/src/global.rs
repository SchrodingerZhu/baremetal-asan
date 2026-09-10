//! Startup poisoning of global descriptors emitted into a linker section.

use crate::platform::Platform;
use core::{ffi::c_void, mem, slice};

/// LLVM's `__asan_global` descriptor: eight pointer-sized fields in ABI order.
/// Only the address and sizes are used for SRAM initialization; diagnostic and
/// dynamic-initialization metadata are retained without dereferencing pointers.
#[repr(C)]
pub struct Global {
    pub beg: usize,
    pub size: usize,
    pub size_with_redzone: usize,
    pub name: *const core::ffi::c_char,
    pub module_name: *const core::ffi::c_char,
    pub has_dynamic_init: usize,
    pub gcc_location: *const c_void,
    pub odr_indicator: usize,
}

/// Initialize global payload and redzone shadow within application SRAM.
/// Globals outside the platform's application range, including flash constants,
/// are ignored. A partial final payload granule records its valid prefix; all
/// remaining redzone granules receive global poison (`0xf9`).
///
/// # Safety
/// Run during startup with exclusive access to initialized shadow RAM. Each
/// descriptor must describe an independently reserved global and its redzone;
/// their granules must not overlap other live storage or the descriptor table.
pub unsafe fn init<P: Platform>(globals: &[Global]) {
    const { assert!(P::GRANULE <= 128) };
    for global in globals {
        let Some(end) = global.beg.checked_add(global.size_with_redzone) else {
            continue;
        };
        if global.beg < P::APPLICATION.start
            || end > P::APPLICATION.end
            || global.size > global.size_with_redzone
            || global.beg % P::GRANULE != 0
            || global.size_with_redzone % P::GRANULE != 0
        {
            continue;
        }
        let payload_end = global.beg + global.size;
        let tail = global.size % P::GRANULE;
        let redzone = payload_end - tail;
        // SAFETY: startup owns this global's shadow; the entire reservation is
        // validated above. The platform splits writes at physical boundaries.
        unsafe { P::to_shadow_slices_mut(global.beg, global.size) }
            .for_each(|part| part.bytes.fill(0));
        unsafe { P::to_shadow_slices_mut(redzone, end - redzone) }.for_each(|part| {
            part.bytes.fill(0xf9u8 as i8);
            if tail != 0 && part.memory.start == redzone {
                part.bytes[0] = tail as i8;
            }
        });
    }
}

/// Initialize descriptors between linker-provided section bounds.
/// Empty, null, reversed, or misaligned bounds are ignored.
///
/// # Safety
/// A nonempty accepted range must contain readable, contiguous `Global` records
/// without padding between records. The shadow ownership requirements of `init`
/// apply. The end pointer is exclusive.
pub unsafe fn init_from_range<P: Platform>(start: *const Global, end: *const Global) {
    let Some(bytes) = (end as usize).checked_sub(start as usize) else {
        return;
    };
    if bytes == 0
        || start.is_null()
        || start as usize % mem::align_of::<Global>() != 0
        || bytes % mem::size_of::<Global>() != 0
        || bytes > isize::MAX as usize
    {
        return;
    }
    // SAFETY: the caller supplies the readable descriptor table and exclusive
    // initialized shadow; the bounds above form a whole number of ABI records.
    unsafe {
        init::<P>(slice::from_raw_parts(
            start,
            bytes / mem::size_of::<Global>(),
        ))
    };
}

/// Export explicit startup initialization and compatibility registration stubs.
/// Firmware calls `__asan_init_globals(start, end)` after initializing shadow.
/// Automatic registration, unloading, and initialization-order checks stay stubbed.
#[macro_export]
macro_rules! export_asan_globals {
    ($platform:ty) => {
        /// # Safety
        /// The section must contain readable LLVM global descriptors, and startup
        /// must provide exclusive access to their initialized shadow.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_init_globals(
            start: *const $crate::global::Global,
            end: *const $crate::global::Global,
        ) {
            unsafe { $crate::global::init_from_range::<$platform>(start, end) };
        }

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
