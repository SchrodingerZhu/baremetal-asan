//! Raw allocation from LLVM libc's rotating free stores.
//!
//! The platform's arena holds both allocator state and allocation storage. An
//! Embassy critical-section mutex protects initialization, allocation, and free,
//! including rotation when the active store cannot satisfy an allocation.
//! [`malloc`] and [`free`] add ASan redzones and shadow updates. Fake-stack
//! frames share this allocator and its quarantine through `stack::fake_stack`.

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

mod sanitized;
pub(crate) mod shadow;
pub use sanitized::{free, malloc};

pub(crate) struct Allocator(NonNull<c_void>);

// SAFETY: this handle uniquely owns the arena. Heap serializes every access to
// its allocator state; allocations themselves are disjoint from that state.
unsafe impl Send for Allocator {}

impl Allocator {
    unsafe fn new<P: Platform>() -> Option<Self> {
        let base = P::alloc_base();
        let size = P::alloc_size();
        let end = base.checked_add(size)?;
        if base == 0
            || size == 0
            || size > isize::MAX as usize
            || base < P::APPLICATION.start
            || end > P::APPLICATION.end
        {
            return None;
        }
        // SAFETY: the caller grants exclusive static ownership of the arena.
        NonNull::new(unsafe { __baremetal_asan_heap_init(base as *mut c_void, size) }).map(Self)
    }

    pub(crate) fn allocate(&mut self, layout: Layout) -> Option<NonNull<u8>> {
        // SAFETY: this handle is initialized and exclusively borrowed.
        NonNull::new(unsafe {
            __baremetal_asan_heap_allocate(self.0.as_ptr(), layout.size(), layout.align()).cast()
        })
    }

    pub(crate) unsafe fn deallocate(&mut self, ptr: NonNull<u8>) {
        // SAFETY: the caller supplies this allocator's live allocation.
        unsafe { __baremetal_asan_heap_deallocate(self.0.as_ptr(), ptr.as_ptr().cast()) };
    }
}

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
            // SAFETY: the caller reserves the platform's arena for this heap.
            *state = unsafe { Allocator::new::<P>() };
            state.is_some()
        })
    }

    /// Allocate uninitialized memory, rotating quarantine on allocation pressure.
    /// Returns `None` before initialization, for zero size, or on exhaustion.
    pub fn allocate(&self, layout: Layout) -> Option<NonNull<u8>> {
        self.state.lock(|state| {
            let mut state = state.borrow_mut();
            state.as_mut()?.allocate(layout)
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
            unsafe { allocator.deallocate(ptr) };
        });
    }

    /// Keep native allocation and shadow updates in one critical section.
    ///
    /// # Safety
    /// The caller reserves the platform arena as for `init`. Any shadow updates
    /// in `operation` additionally require initialized, exclusively held shadow.
    pub(crate) unsafe fn with_initialized<R>(
        &self,
        operation: impl FnOnce(&mut Allocator) -> R,
    ) -> Option<R> {
        self.state.lock(|state| {
            let mut state = state.borrow_mut();
            if state.is_none() {
                // SAFETY: the caller grants exclusive ownership of the arena.
                *state = unsafe { Allocator::new::<P>() };
            }
            state.as_mut().map(operation)
        })
    }
}

impl<P: Platform> Default for Heap<P> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;

/// Export malloc/free wrappers using a shared `Heap<Platform>` static.
#[macro_export]
macro_rules! export_asan_heap {
    ($platform:ty, $heap:ident $(,)?) => {
        /// # Safety
        /// The platform arena and initialized shadow must be reserved for this runtime.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_malloc(size: usize) -> *mut ::core::ffi::c_void {
            unsafe { $crate::heap::malloc::<$platform>(&$heap, size).cast() }
        }

        /// # Safety
        /// `ptr` must be null or a live pointer returned by this runtime's malloc.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn __asan_free(ptr: *mut ::core::ffi::c_void) {
            unsafe { $crate::heap::free::<$platform>(&$heap, ptr.cast()) };
        }
    };
}
