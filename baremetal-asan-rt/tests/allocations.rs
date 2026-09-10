use baremetal_asan_rt::{self as runtime, heap, platform::Platform, stack::fake_stack};
use core::{cell::UnsafeCell, ops::Range};
use runtime::platform::{Shadow, map_region};

const ARENA_SIZE: usize = 256 * 1024;

struct Storage<T>(UnsafeCell<T>);
// Each test owns separate static storage. Only the runtime and the test access it.
unsafe impl<T> Sync for Storage<T> {}

#[repr(align(65536))]
struct Arena(Storage<[u8; ARENA_SIZE]>);

macro_rules! fixture {
    ($name:ident, $scale:literal) => {
        mod $name {
            use super::*;
            static ARENA: Arena = Arena(Storage(UnsafeCell::new([0; ARENA_SIZE])));
            // A split inside an allocation exercises logical-to-physical mapping.
            static LEFT: Storage<[i8; 2051]> = Storage(UnsafeCell::new([0; 2051]));
            static RIGHT: Storage<[i8; ARENA_SIZE / (1 << $scale) - 2051]> =
                Storage(UnsafeCell::new([0; ARENA_SIZE / (1 << $scale) - 2051]));

            pub struct TestPlatform;
            impl Platform for TestPlatform {
                const APPLICATION: Range<usize> = 0..usize::MAX;
                const SHADOW_SCALE: u32 = $scale;
                const SHADOW_BASE: usize = 0;
                fn alloc_base() -> usize {
                    ARENA.0.0.get().cast::<u8>() as usize
                }
                fn alloc_size() -> usize {
                    ARENA_SIZE
                }
                fn to_writable_shadow_ranges(
                    addr: usize,
                    size: usize,
                ) -> impl Iterator<Item = Shadow<Range<usize>>> {
                    let last = size.checked_sub(1).and_then(|n| addr.checked_add(n));
                    let base = Self::alloc_base();
                    let split = base + 2051 * Self::GRANULE;
                    map_region::<Self>(addr, last, base..split, LEFT.0.get() as usize)
                        .into_iter()
                        .chain(map_region::<Self>(
                            addr,
                            last,
                            split..base + ARENA_SIZE,
                            RIGHT.0.get() as usize,
                        ))
                }
            }
        }
    };
}

fixture!(granule8, 3);
fixture!(granule16, 4);
runtime::export_asan!(granule8::TestPlatform);

unsafe fn shadow<P: Platform>(addr: usize) -> i8 {
    unsafe { P::to_shadow_slices(addr, 1) }
        .next()
        .unwrap()
        .bytes[0]
}

unsafe fn frame<P: Platform, const CLASS: usize>(heap: &heap::Heap<P>) {
    let bytes = 64 << CLASS;
    let size = bytes - P::GRANULE;
    unsafe {
        let addr = fake_stack::allocate::<P, CLASS>(heap, size);
        assert_ne!(addr, 0);
        assert_eq!(addr % bytes, 0);
        assert_eq!(shadow::<P>(addr), 0);
        assert_eq!(shadow::<P>(addr + size), 0xf3u8 as i8);
        runtime::access::check_range::<P>(addr, size, true);
        (addr as *mut u8).write_bytes(0x45, size);
        fake_stack::deallocate::<P, CLASS>(heap, addr);
        assert!(
            P::to_shadow_slices(addr, bytes)
                .flat_map(|part| part.bytes)
                .all(|&byte| byte == 0xf5u8 as i8)
        );
    }
}

unsafe fn exercise<P: Platform>(heap: &heap::Heap<P>) {
    unsafe {
        for size in [0, 1, 7, 8, 9, 15, 16, 17, 31, 32, 33, 20_001] {
            let ptr = heap::malloc(heap, size);
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % 16, 0);
            assert_eq!(shadow::<P>(ptr as usize - 1), 0xfau8 as i8);
            if size != 0 {
                runtime::access::check_range::<P>(ptr as usize, size, true);
                ptr.write_bytes(0x5a, size);
                let tail = size % P::GRANULE;
                assert_eq!(shadow::<P>(ptr as usize + size - 1), tail as i8);
            }
            // The checker must reject the first byte beyond the requested size,
            // including zero-sized allocations and partial final granules.
            assert!(
                std::panic::catch_unwind(|| {
                    runtime::access::check_range::<P>(ptr as usize + size, 1, false)
                })
                .is_err()
            );
            heap::free(heap, ptr);
            assert_eq!(shadow::<P>(ptr as usize), 0xfdu8 as i8);
        }
        assert!(heap::malloc(heap, usize::MAX).is_null());
        heap::free(heap, core::ptr::null_mut());

        macro_rules! classes {
            ($($class:literal),*) => { $(frame::<P, $class>(heap);)* };
        }
        classes!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);

        // Keep one malloc object live through thousands of frame retirements.
        // Capacity forces native rotations; no second frame quarantine exists.
        let live = heap::malloc(heap, 33);
        assert!(!live.is_null());
        live.write_bytes(0x6b, 33);
        let mut previous = 0;
        for _ in 0..8192 {
            let addr = fake_stack::allocate::<P, 0>(heap, 48);
            assert_ne!(addr, 0);
            assert_ne!(addr, previous);
            assert_eq!(shadow::<P>(addr), 0);
            fake_stack::deallocate::<P, 0>(heap, addr);
            previous = addr;
        }
        assert!(
            core::slice::from_raw_parts(live, 33)
                .iter()
                .all(|&x| x == 0x6b)
        );
        runtime::access::check_range::<P>(live as usize, 33, false);
        heap::free(heap, live);

        // Both allocation types return storage to the same native heap.
        let large = heap::malloc(heap, 200 * 1024);
        assert!(!large.is_null());
        heap::free(heap, large);
        assert_eq!(fake_stack::allocate::<P, 0>(heap, 65), 0);
        fake_stack::deallocate::<P, 0>(heap, 0);
    }
}

#[test]
fn granule_eight_and_exported_abi() {
    unsafe {
        exercise(&__ASAN_HEAP);
        __asan_option_detect_stack_use_after_return = 0;
        assert_eq!(__asan_stack_malloc_0(48), 0);
        let frame = __asan_stack_malloc_always_0(48);
        assert_ne!(frame, 0);
        __asan_stack_free_0(frame, 48);
        __asan_option_detect_stack_use_after_return = 1;
        let frame = __asan_stack_malloc_0(48);
        assert_ne!(frame, 0);
        __asan_stack_free_0(frame, 48);
        let ptr = __asan_malloc(17);
        assert!(!ptr.is_null());
        __asan_store1(ptr as usize + 16);
        __asan_free(ptr);
    }
}

#[test]
fn granule_sixteen() {
    unsafe { exercise(&heap::Heap::<granule16::TestPlatform>::new()) };
}
