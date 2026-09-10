extern crate std;

use super::*;
use core::{cell::UnsafeCell, mem::MaybeUninit, ops::Range, slice};
use std::{thread, vec, vec::Vec};

#[repr(align(64))]
struct Arena<const N: usize>(UnsafeCell<[MaybeUninit<u8>; N]>);

// Each test has a separate static arena. After init, only its heap and that
// heap's live allocations access the bytes, including in the concurrent test.
unsafe impl<const N: usize> Sync for Arena<N> {}

macro_rules! test_heap {
    ($size:expr) => {{
        static ARENA: Arena<$size> = Arena(UnsafeCell::new([MaybeUninit::uninit(); $size]));
        struct TestPlatform;
        impl Platform for TestPlatform {
            const APPLICATION: Range<usize> = 0..usize::MAX;
            const SHADOW_SCALE: u32 = 3;
            const SHADOW_BASE: usize = 0;
            fn alloc_base() -> usize {
                ARENA.0.get().cast::<u8>() as usize
            }
            fn alloc_size() -> usize {
                $size
            }
        }
        Heap::<TestPlatform>::new()
    }};
}

#[test]
fn initialization_is_explicit_and_never_resets_live_allocations() {
    let heap = test_heap!(4096);
    let layout = Layout::from_size_align(64, 16).unwrap();
    assert!(heap.allocate(layout).is_none());
    unsafe {
        assert!(heap.init());
        let ptr = heap.allocate(layout).unwrap();
        ptr.as_ptr().write_bytes(0x5a, layout.size());
        assert!(!heap.init());
        assert!(
            slice::from_raw_parts(ptr.as_ptr(), layout.size())
                .iter()
                .all(|&x| x == 0x5a)
        );
        heap.deallocate(ptr);
    }
}

#[test]
fn absent_and_too_small_arenas_stay_uninitialized() {
    let empty = test_heap!(0);
    let tiny = test_heap!(1);
    unsafe {
        assert!(!empty.init());
        assert!(!tiny.init());
    }
    let layout = Layout::new::<u64>();
    assert!(empty.allocate(layout).is_none());
    assert!(tiny.allocate(layout).is_none());
}

#[test]
fn freed_blocks_are_reused_only_after_allocation_pressure() {
    let heap = test_heap!(4096);
    let layout = Layout::from_size_align(64, 16).unwrap();
    unsafe { assert!(heap.init()) };
    let retired = heap.allocate(layout).unwrap();
    unsafe { heap.deallocate(retired) };
    let next = heap.allocate(layout).unwrap();
    assert_ne!(next, retired);

    let mut live = vec![next];
    while let Some(ptr) = heap.allocate(layout) {
        assert!(!live.contains(&ptr));
        live.push(ptr);
        assert!(live.len() <= 4096 / layout.size());
    }
    assert!(live.contains(&retired));
    for ptr in live {
        unsafe { heap.deallocate(ptr) };
    }
}

#[test]
fn rotation_coalesces_neighbors_from_both_indexes_without_losing_capacity() {
    let heap = test_heap!(16384);
    let small = Layout::from_size_align(128, 16).unwrap();
    let large = Layout::from_size_align(12288, 16).unwrap();
    unsafe { assert!(heap.init()) };
    for _ in 0..16 {
        let mut blocks = Vec::new();
        while let Some(ptr) = heap.allocate(small) {
            unsafe { ptr.as_ptr().write_bytes(0xa7, small.size()) };
            blocks.push(ptr);
        }
        for &ptr in blocks.iter().step_by(2) {
            unsafe { heap.deallocate(ptr) };
        }
        // Rotation makes these separated holes available, but live neighbors
        // prevent a large allocation. Those live bytes must survive the failure.
        assert!(heap.allocate(large).is_none());
        for &ptr in blocks.iter().skip(1).step_by(2) {
            unsafe {
                assert!(
                    slice::from_raw_parts(ptr.as_ptr(), small.size())
                        .iter()
                        .all(|&x| x == 0xa7)
                );
                heap.deallocate(ptr);
            }
        }
        // Every byte is free, but alternating holes belong to different indexes.
        // Transferring leftovers must recover one contiguous free block.
        let ptr = heap.allocate(large).unwrap();
        unsafe { heap.deallocate(ptr) };
    }
}

#[test]
fn alignment_zero_size_and_impossible_requests_preserve_live_blocks() {
    let heap = test_heap!(16384);
    unsafe { assert!(heap.init()) };
    assert!(
        heap.allocate(Layout::from_size_align(0, 16).unwrap())
            .is_none()
    );
    let mut live = Vec::new();
    for align in [1, 2, 8, 16, 32, 64, 256, 1024] {
        for size in [1, 17, 123] {
            let layout = Layout::from_size_align(size, align).unwrap();
            let ptr = heap.allocate(layout).unwrap();
            assert_eq!(ptr.as_ptr() as usize % align, 0);
            unsafe { ptr.as_ptr().write_bytes(0x39, size) };
            live.push((ptr, layout));
        }
    }
    assert!(
        heap.allocate(Layout::from_size_align(isize::MAX as usize, 1).unwrap())
            .is_none()
    );
    for (ptr, layout) in live {
        unsafe {
            assert!(
                slice::from_raw_parts(ptr.as_ptr(), layout.size())
                    .iter()
                    .all(|&x| x == 0x39)
            );
            heap.deallocate(ptr);
        }
    }
}

struct Allocation {
    ptr: NonNull<u8>,
    layout: Layout,
    byte: u8,
}

impl Allocation {
    fn check(&self) {
        unsafe {
            assert!(
                slice::from_raw_parts(self.ptr.as_ptr(), self.layout.size())
                    .iter()
                    .all(|&x| x == self.byte)
            );
        }
    }
}

#[test]
fn mixed_sizes_survive_many_rotations_and_coalesces() {
    let heap = test_heap!(32768);
    unsafe { assert!(heap.init()) };
    let mut random = 0x97ab_193du32;
    let mut live: [Option<Allocation>; 64] = [const { None }; 64];
    for step in 0..20_000 {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        let slot = random as usize % live.len();
        if let Some(allocation) = live[slot].take() {
            allocation.check();
            unsafe { heap.deallocate(allocation.ptr) };
        } else {
            let size = (random >> 6) as usize % 2048;
            let align = 1 << ((random >> 18) % 9);
            let layout = Layout::from_size_align(size, align).unwrap();
            if let Some(ptr) = heap.allocate(layout) {
                assert_eq!(ptr.as_ptr() as usize % align, 0);
                let start = ptr.as_ptr() as usize;
                for other in live.iter().flatten() {
                    let other_start = other.ptr.as_ptr() as usize;
                    assert!(
                        size == 0
                            || other.layout.size() == 0
                            || start + size <= other_start
                            || other_start + other.layout.size() <= start
                    );
                }
                let byte = step as u8;
                unsafe { ptr.as_ptr().write_bytes(byte, size) };
                live[slot] = Some(Allocation { ptr, layout, byte });
            }
        }
        if step % 64 == 0 {
            live.iter().flatten().for_each(Allocation::check);
        }
    }
    for allocation in live.into_iter().flatten() {
        allocation.check();
        unsafe { heap.deallocate(allocation.ptr) };
    }
    let large = Layout::from_size_align(28 * 1024, 64).unwrap();
    let ptr = heap.allocate(large).unwrap();
    unsafe { heap.deallocate(ptr) };
}

#[test]
fn critical_section_serializes_concurrent_heap_users() {
    let heap = test_heap!(16384);
    unsafe { assert!(heap.init()) };
    thread::scope(|scope| {
        for worker in 0..4 {
            let heap = &heap;
            scope.spawn(move || {
                for step in 0..1000 {
                    let layout = Layout::from_size_align(16 + (step % 256), 16).unwrap();
                    let ptr = heap.allocate(layout).unwrap();
                    let allocation = Allocation {
                        ptr,
                        layout,
                        byte: worker,
                    };
                    unsafe { ptr.as_ptr().write_bytes(worker, layout.size()) };
                    thread::yield_now();
                    allocation.check();
                    unsafe { heap.deallocate(ptr) };
                }
            });
        }
    });
}

#[test]
fn ffi_aligns_state_inside_the_arena_and_rejects_invalid_requests() {
    for offset in 0..32 {
        let mut arena = vec![0xa5u8; 8192 + 64];
        let base = unsafe { arena.as_mut_ptr().add(offset) };
        unsafe {
            let heap = __baremetal_asan_heap_init(base.cast(), 8192);
            assert!(!heap.is_null());
            assert!(__baremetal_asan_heap_allocate(heap, 1, 0).is_null());
            assert!(__baremetal_asan_heap_allocate(heap, 1, 3).is_null());
            assert!(__baremetal_asan_heap_allocate(heap, usize::MAX, 16).is_null());
            assert!(__baremetal_asan_heap_allocate(heap, 0, 16).is_null());
            let ptr = __baremetal_asan_heap_allocate(heap, 127, 64).cast::<u8>();
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % 64, 0);
            assert!(ptr as usize >= base as usize);
            assert!(ptr as usize + 127 <= base as usize + 8192);
            ptr.write_bytes(0x39, 127);
            __baremetal_asan_heap_deallocate(heap, ptr.cast());
            __baremetal_asan_heap_deallocate(heap, core::ptr::null_mut());
        }
        assert!(arena[..offset].iter().all(|&byte| byte == 0xa5));
        assert!(arena[offset + 8192..].iter().all(|&byte| byte == 0xa5));
    }
    unsafe {
        assert!(__baremetal_asan_heap_init(core::ptr::null_mut(), 8192).is_null());
        assert!(__baremetal_asan_heap_init((usize::MAX - 15) as *mut _, 32).is_null());
        assert!(__baremetal_asan_heap_allocate(core::ptr::null_mut(), 64, 16).is_null());
    }
}
