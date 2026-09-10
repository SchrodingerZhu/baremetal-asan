//! LLVM fake-stack frames allocated directly from the rotational heap.
//!
//! Compile with `-mllvm -asan-max-inline-poisoning-size=0` so every return calls
//! `__asan_stack_free_*`. The compiler's inline flag-store retirement protocol
//! is not used. LLVM writes frame headers and live stack redzones itself.

use crate::{heap::Heap, heap::shadow, platform::Platform};
use core::{alloc::Layout, ptr::NonNull};

/// Allocate one of LLVM's 64-byte through 64-KiB frame classes.
/// The full class alignment also satisfies over-aligned objects in the frame.
/// A zero result tells LLVM to fall back to the real stack.
///
/// # Safety
/// The heap arena and initialized shadow must be reserved for this runtime.
pub unsafe fn allocate<P: Platform, const CLASS: usize>(heap: &Heap<P>, size: usize) -> usize {
    if CLASS > 10 || size == 0 || size > (64 << CLASS) {
        return 0;
    }
    let bytes = 64 << CLASS;
    let layout = Layout::from_size_align(bytes, bytes.max(P::GRANULE)).unwrap();
    // SAFETY: the caller provides arena/shadow ownership. Frame preparation is
    // atomic with respect to other heap users; no second quarantine is needed.
    unsafe {
        heap.with_initialized(|allocator| {
            let frame = allocator.allocate(layout)?;
            let addr = frame.as_ptr() as usize;
            shadow::poison::<P>(addr, bytes, 0xf3);
            shadow::unpoison::<P>(addr, size);
            Some(addr)
        })
    }
    .flatten()
    .unwrap_or(0)
}

/// Poison a returned frame and put it into the heap's existing quarantine.
///
/// # Safety
/// `addr` must be zero or a live frame from this heap and size class. The caller
/// must have finished using it, and shadow must remain initialized.
pub unsafe fn deallocate<P: Platform, const CLASS: usize>(heap: &Heap<P>, addr: usize) {
    let Some(frame) = NonNull::new(addr as *mut u8) else {
        return;
    };
    // SAFETY: the caller supplies a live frame. Poison it before native free can
    // make it eligible for reuse; both actions hold the same critical section.
    unsafe {
        heap.with_initialized(|allocator| {
            shadow::poison::<P>(addr, 64 << CLASS, 0xf5);
            allocator.deallocate(frame);
        })
    };
}
