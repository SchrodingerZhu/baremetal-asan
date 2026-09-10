use super::{Heap, shadow};
use crate::platform::Platform;
use core::{alloc::Layout, mem::size_of, ptr, ptr::NonNull};

// One size word lives in the poisoned left redzone. The native allocator owns
// all free-list metadata and quarantine; we only need the requested byte count.
fn redzone<P: Platform>() -> usize {
    P::GRANULE.max(16).max(size_of::<usize>())
}

fn layout<P: Platform>(size: usize) -> Option<Layout> {
    let redzone = redzone::<P>();
    let payload = size.max(1).checked_add(redzone - 1)? & !(redzone - 1);
    Layout::from_size_align(payload.checked_add(2 * redzone)?, redzone).ok()
}

/// Allocate a buffer with poisoned redzones and an exact addressable prefix.
/// Returns null on exhaustion/overflow; zero size returns a freeable pointer
/// whose entire storage is poisoned. The heap initializes lazily.
///
/// # Safety
/// The platform arena must be exclusively reserved for this heap, and shadow
/// must be initialized. Allocation and shadow updates run under its mutex.
pub unsafe fn malloc<P: Platform>(heap: &Heap<P>, size: usize) -> *mut u8 {
    let Some(layout) = layout::<P>(size) else {
        return ptr::null_mut();
    };
    // SAFETY: the caller reserves the arena and shadow; each native allocation
    // owns complete granules and is initialized before leaving the mutex.
    unsafe {
        heap.with_initialized(|allocator| {
            let raw = allocator.allocate(layout)?;
            let user = raw.as_ptr().add(redzone::<P>());
            raw.cast::<usize>().as_ptr().write(size);
            shadow::poison::<P>(raw.as_ptr() as usize, layout.size(), 0xfa);
            shadow::unpoison::<P>(user as usize, size);
            Some(user)
        })
    }
    .flatten()
    .unwrap_or(ptr::null_mut())
}

/// Poison a malloc allocation and return it to the native heap's quarantine.
///
/// # Safety
/// `ptr` must be null or a live result of `malloc` on this heap. All accesses
/// must have finished, and the platform's shadow must remain initialized.
pub unsafe fn free<P: Platform>(heap: &Heap<P>, ptr: *mut u8) {
    if ptr.is_null() {
        return;
    }
    // SAFETY: the caller supplies this heap's live malloc result. Its preceding
    // redzone holds our initialized size word; all updates happen under the lock.
    unsafe {
        heap.with_initialized(|allocator| {
            let raw = NonNull::new_unchecked(ptr.sub(redzone::<P>()));
            let size = raw.cast::<usize>().as_ptr().read();
            let layout = layout::<P>(size).expect("invalid ASan allocation size");
            shadow::poison::<P>(raw.as_ptr() as usize, layout.size(), 0xfd);
            allocator.deallocate(raw);
        })
    };
}
