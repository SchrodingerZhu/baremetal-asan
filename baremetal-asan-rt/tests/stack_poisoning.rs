use baremetal_asan_rt::{self as runtime, platform::Platform, stack};
use core::{cell::UnsafeCell, ops::Range};
use runtime::platform::{Shadow, map_region};
use std::sync::Mutex;

struct Storage(UnsafeCell<[i8; 128]>);
// Every test holds LOCK while accessing either shadow region.
unsafe impl Sync for Storage {}
static LOCK: Mutex<()> = Mutex::new(());
static LEFT: Storage = Storage(UnsafeCell::new([0; 128]));
static RIGHT: Storage = Storage(UnsafeCell::new([0; 128]));

struct Split<const SCALE: u32>;
impl<const SCALE: u32> Platform for Split<SCALE> {
    const APPLICATION: Range<usize> = 0x1000..0x1400;
    const SHADOW_SCALE: u32 = SCALE;
    const SHADOW_BASE: usize = 0;

    fn to_shadow_ranges(addr: usize, size: usize) -> impl Iterator<Item = Shadow<Range<usize>>> {
        let last = size.checked_sub(1).and_then(|n| addr.checked_add(n));
        // Granule-aligned for both scales, but inside a 32-byte alloca redzone.
        map_region::<Self>(addr, last, 0x1000..0x1210, LEFT.0.get() as usize)
            .into_iter()
            .chain(map_region::<Self>(
                addr,
                last,
                0x1210..0x1400,
                RIGHT.0.get() as usize,
            ))
    }
}

runtime::export_asan_stack!(Split<3>);

unsafe fn reset(value: i8) {
    unsafe {
        LEFT.0.get().write([value; 128]);
        RIGHT.0.get().write([value; 128]);
    }
}

unsafe fn byte<P: Platform>(addr: usize) -> i8 {
    unsafe { P::to_shadow_slices(addr, 1) }
        .next()
        .unwrap()
        .bytes[0]
}

unsafe fn set_byte<P: Platform>(addr: usize, value: i8) {
    unsafe { P::to_shadow_slices_mut(addr, 1) }
        .next()
        .unwrap()
        .bytes[0] = value;
}

unsafe fn lifetime<P: Platform>() {
    // End the object's partial granule in the second physical region.
    let addr = 0x1200;
    let size = 2 * P::GRANULE + 3;
    let last = addr + size - 1;
    unsafe {
        reset(0xf3u8 as i8);
        stack::unpoison_stack_memory::<P>(addr, size);
        assert_eq!(byte::<P>(addr), 0);
        assert_eq!(byte::<P>(last), 3);
        runtime::access::check_range::<P>(addr, size, false);

        stack::poison_stack_memory::<P>(addr, size);
        assert_eq!(byte::<P>(addr), 0xf8u8 as i8);
        assert_eq!(byte::<P>(last), 0xf8u8 as i8);
        assert!(
            std::panic::catch_unwind(|| { runtime::access::check_range::<P>(addr, size, false) })
                .is_err()
        );
        stack::unpoison_stack_memory::<P>(addr, size);
        assert_eq!(byte::<P>(last), 3);
        assert_eq!(byte::<P>(addr - 1), 0xf3u8 as i8);
        assert_eq!(byte::<P>(last + P::GRANULE), 0xf3u8 as i8);

        // Poisoning this prefix cannot invalidate a live suffix in the same
        // granule. Unpoisoning must not shorten that suffix either.
        for neighbor in [0, 5] {
            set_byte::<P>(last, neighbor);
            stack::poison_stack_memory::<P>(addr, size);
            assert_eq!(byte::<P>(last), neighbor);
            stack::unpoison_stack_memory::<P>(addr, size);
            assert_eq!(byte::<P>(last), neighbor);
        }
        set_byte::<P>(last, 0xf3u8 as i8);
        stack::poison_stack_memory::<P>(addr, size);
        assert_eq!(byte::<P>(last), 0xf3u8 as i8);
    }
}

#[test]
fn lifetime_hooks_preserve_partial_granules_and_live_neighbors() {
    let _guard = LOCK.lock().unwrap();
    unsafe {
        lifetime::<Split<3>>();
        lifetime::<Split<4>>();
    }
}

unsafe fn dynamic_redzones<P: Platform>() {
    unsafe {
        for addr in [0x1100, 0x1200, 0x1220] {
            for size in 0..=97 {
                reset(0);
                stack::alloca_poison::<P>(addr, size);
                let end = (addr + size).next_multiple_of(32) + 32;
                // Independent byte-level bounds check across payload, padding,
                // and both redzones, including split and partial granules.
                for app in addr - 32..end {
                    let shadow = byte::<P>(app);
                    let accessible =
                        shadow == 0 || (shadow > 0 && app % P::GRANULE < shadow as usize);
                    assert_eq!(accessible, (addr..addr + size).contains(&app));
                }
                assert_eq!(byte::<P>(addr - 32), 0xcau8 as i8);
                assert_eq!(byte::<P>(end - 1), 0xcbu8 as i8);
                assert_eq!(byte::<P>(addr - 33), 0);
                assert_eq!(byte::<P>(end), 0);
            }
        }
    }
}

#[test]
fn dynamic_allocas_have_exact_bounds_and_32_byte_redzones() {
    let _guard = LOCK.lock().unwrap();
    unsafe {
        dynamic_redzones::<Split<3>>();
        dynamic_redzones::<Split<4>>();
    }
}

unsafe fn cleanup<P: Platform>() {
    unsafe {
        reset(0xf8u8 as i8);
        stack::allocas_unpoison::<P>(0x1180, 0x1283);
        assert_eq!(byte::<P>(0x117f), 0xf8u8 as i8);
        assert!(
            P::to_shadow_slices(0x1180, 0x100)
                .flat_map(|part| part.bytes)
                .all(|&value| value == 0)
        );
        assert_eq!(byte::<P>(0x1280), 0xf8u8 as i8);
    }
}

#[test]
fn cleanup_crosses_regions_without_clearing_the_partial_upper_granule() {
    let _guard = LOCK.lock().unwrap();
    unsafe {
        cleanup::<Split<3>>();
        cleanup::<Split<4>>();
    }
}

#[test]
fn empty_unsupported_and_wrapping_requests_leave_shadow_alone() {
    let _guard = LOCK.lock().unwrap();
    unsafe {
        reset(5);
        for (addr, size) in [(0x1200, 0), (0x2000, 32), (usize::MAX - 31, 64)] {
            stack::poison_stack_memory::<Split<3>>(addr, size);
            stack::unpoison_stack_memory::<Split<3>>(addr, size);
        }
        for (addr, size) in [(0, 1), (0x2000, 32), (usize::MAX - 31, 64)] {
            stack::alloca_poison::<Split<3>>(addr, size);
        }
        for (top, bottom) in [(0, 0x1300), (0x1200, 0x1200), (0x1300, 0x1200)] {
            stack::allocas_unpoison::<Split<3>>(top, bottom);
        }
        assert_eq!(*LEFT.0.get(), [5; 128]);
        assert_eq!(*RIGHT.0.get(), [5; 128]);
    }
}

#[test]
fn compiler_abi_exports_update_shadow() {
    let _guard = LOCK.lock().unwrap();
    unsafe {
        reset(0);
        __asan_alloca_poison(0x1200, 19);
        assert_eq!(byte::<Split<3>>(0x11ff), 0xcau8 as i8);
        assert_eq!(byte::<Split<3>>(0x1212), 3);
        __asan_poison_stack_memory(0x1200, 19);
        assert_eq!(byte::<Split<3>>(0x1212), 0xf8u8 as i8);
        __asan_unpoison_stack_memory(0x1200, 19);
        assert_eq!(byte::<Split<3>>(0x1212), 3);
        __asan_allocas_unpoison(0x11e0, 0x1240);
        assert!(
            Split::<3>::to_shadow_slices(0x11e0, 0x60)
                .flat_map(|part| part.bytes)
                .all(|&value| value == 0)
        );
    }
}
