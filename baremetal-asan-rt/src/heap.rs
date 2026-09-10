//! Raw allocation from LLVM libc's rotating free stores.
//!
//! The platform's arena holds both allocator state and allocation storage. An
//! Embassy critical-section mutex protects initialization, allocation, and free,
//! including rotation when the active store cannot satisfy an allocation.
//! Callers manage ASan redzones and shadow poisoning separately.

use crate::platform::Platform;
use core::{alloc::Layout, cell::RefCell, ffi::c_void, marker::PhantomData, ptr::NonNull};
use embassy_sync::blocking_mutex::CriticalSectionMutex;

unsafe extern "C" {
    fn __baremetal_asan_heap_init(base: *mut c_void, size: usize) -> *mut c_void;
    fn __baremetal_asan_heap_allocate(
        heap: *mut c_void,
        size: usize,
        alignment: usize,
    ) -> *mut c_void;
    fn __baremetal_asan_heap_deallocate(heap: *mut c_void, ptr: *mut c_void);
}

struct Allocator(NonNull<c_void>);

// SAFETY: this handle uniquely owns the arena. Heap serializes every access to
// its allocator state; allocations themselves are disjoint from that state.
unsafe impl Send for Allocator {}

/// A platform's reserved heap, shared between threads and interrupts.
///
/// The target must provide a `critical-section` backend. Heap initialization
/// reserves part of the arena for LLVM libc's state; allocations use the rest.
pub struct Heap<P: Platform> {
    state: CriticalSectionMutex<RefCell<Option<Allocator>>>,
    platform: PhantomData<fn() -> P>,
}

impl<P: Platform> Heap<P> {
    pub const fn new() -> Self {
        Self {
            state: CriticalSectionMutex::new(RefCell::new(None)),
            platform: PhantomData,
        }
    }

    /// Initialize from the platform's allocation base and size.
    /// Returns false for an absent, wrapping, out-of-range, or too-small arena,
    /// or if already initialized. Repeated calls never reset live allocations.
    ///
    /// # Safety
    /// The arena must be writable and reserved exclusively for this heap and its
    /// allocations for the rest of the program. It must not overlap static data,
    /// stack, physical shadow, or any other heap's arena.
    pub unsafe fn init(&self) -> bool {
        self.state.lock(|state| {
            let mut state = state.borrow_mut();
            if state.is_some() {
                return false;
            }
            let base = P::alloc_base();
            let size = P::alloc_size();
            let Some(end) = base.checked_add(size) else {
                return false;
            };
            if base == 0
                || size == 0
                || size > isize::MAX as usize
                || base < P::APPLICATION.start
                || end > P::APPLICATION.end
            {
                return false;
            }
            // SAFETY: the caller grants exclusive static ownership of the arena;
            // the FFI aligns and constructs its state within these bounds.
            let handle = unsafe { __baremetal_asan_heap_init(base as *mut c_void, size) };
            *state = NonNull::new(handle).map(Allocator);
            state.is_some()
        })
    }

    /// Allocate uninitialized memory, rotating quarantine on allocation pressure.
    /// Returns `None` before initialization, for zero size, or on exhaustion.
    pub fn allocate(&self, layout: Layout) -> Option<NonNull<u8>> {
        self.state.lock(|state| {
            let mut state = state.borrow_mut();
            let allocator = state.as_mut()?;
            // SAFETY: initialization established a valid handle, and the mutex
            // grants exclusive access. Layout supplies a power-of-two alignment.
            NonNull::new(unsafe {
                __baremetal_asan_heap_allocate(allocator.0.as_ptr(), layout.size(), layout.align())
                    .cast()
            })
        })
    }

    /// Return a block to quarantine until allocation pressure triggers rotation.
    ///
    /// # Safety
    /// `ptr` must be a live allocation from this heap. All accesses to the block
    /// must finish before this call. The original layout is not needed.
    pub unsafe fn deallocate(&self, ptr: NonNull<u8>) {
        self.state.lock(|state| {
            let mut state = state.borrow_mut();
            let allocator = state.as_mut().expect("heap not initialized");
            // SAFETY: the caller supplies this heap's live allocation, and the
            // mutex grants exclusive access to allocator metadata.
            unsafe { __baremetal_asan_heap_deallocate(allocator.0.as_ptr(), ptr.as_ptr().cast()) };
        });
    }
}

impl<P: Platform> Default for Heap<P> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
