use baremetal_asan_rt::{
    self as runtime,
    global::{self, Global},
    platform::{Platform, Shadow, map_region},
};
use core::{cell::UnsafeCell, mem, ops::Range, ptr};
use std::sync::Mutex;

struct Storage(UnsafeCell<[i8; 128]>);
// All tests hold LOCK while accessing shadow.
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

runtime::export_asan_globals!(Split<3>);

fn descriptor(beg: usize, size: usize, size_with_redzone: usize) -> Global {
    Global {
        beg,
        size,
        size_with_redzone,
        name: ptr::null(),
        module_name: ptr::null(),
        has_dynamic_init: 0,
        gcc_location: ptr::null(),
        odr_indicator: 0,
    }
}

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

unsafe fn exact_bounds<P: Platform>() {
    for beg in [0x1080, 0x1200, 0x1220] {
        for size in 0usize..=40 {
            let extent = size.next_multiple_of(P::GRANULE) + 2 * P::GRANULE;
            let globals = [descriptor(beg, size, extent)];
            unsafe {
                reset(5);
                global::init::<P>(&globals);
                // Repeating initialization must reproduce the same shadow.
                global::init::<P>(&globals);
                for addr in beg..beg + extent {
                    let shadow = byte::<P>(addr);
                    let accessible =
                        shadow == 0 || (shadow > 0 && addr % P::GRANULE < shadow as usize);
                    assert_eq!(accessible, addr < beg + size, "size={size}, addr={addr:#x}");
                }
                assert_eq!(byte::<P>(beg + extent - 1), 0xf9u8 as i8);
                assert_eq!(byte::<P>(beg - 1), 5);
                assert_eq!(byte::<P>(beg + extent), 5);
            }
        }
    }
}

#[test]
fn payload_partial_granules_and_redzones_cross_physical_regions() {
    let _guard = LOCK.lock().unwrap();
    unsafe {
        exact_bounds::<Split<3>>();
        exact_bounds::<Split<4>>();
    }
}

#[test]
fn section_abi_initializes_multiple_records_and_checks_detect_oob() {
    let _guard = LOCK.lock().unwrap();
    assert_eq!(mem::size_of::<Global>(), 8 * mem::size_of::<usize>());
    let globals = [
        descriptor(0x1080, 16, 32),
        descriptor(0x1200, 19, 64),
        // A flash-like address and unreadable diagnostic pointers are ignored.
        Global {
            name: ptr::without_provenance(1),
            ..descriptor(0x8000, 4, 32)
        },
    ];
    unsafe {
        reset(0);
        __asan_init_globals(globals.as_ptr(), globals.as_ptr().add(globals.len()));
        assert_eq!(byte::<Split<3>>(0x1090), 0xf9u8 as i8);
        assert_eq!(byte::<Split<3>>(0x1210), 3);
        runtime::access::check_range::<Split<3>>(0x1200, 19, false);
        for (addr, write) in [(0x1213, false), (0x1218, true)] {
            assert!(
                std::panic::catch_unwind(|| runtime::access::check_range::<Split<3>>(
                    addr, 1, write
                ))
                .is_err()
            );
        }
    }
}

#[test]
fn unsupported_and_malformed_reservations_leave_shadow_untouched() {
    let _guard = LOCK.lock().unwrap();
    let globals = [
        descriptor(0, 1, 32),
        descriptor(0x2000, 8, 32),
        descriptor(0x0ff0, 32, 64),
        descriptor(0x13f0, 16, 32),
        descriptor(usize::MAX - 7, 1, 16),
        descriptor(0x1100, 33, 32),
        descriptor(0x1101, 8, 32),
        descriptor(0x1100, 8, 31),
    ];
    unsafe {
        reset(5);
        global::init::<Split<3>>(&globals);
        __asan_init_globals(ptr::null(), ptr::null());
        __asan_init_globals(ptr::null(), globals.as_ptr());
        __asan_init_globals(globals.as_ptr().add(1), globals.as_ptr());
        __asan_init_globals(globals.as_ptr(), globals.as_ptr());
        __asan_init_globals(globals.as_ptr(), globals.as_ptr().byte_add(1));
        __asan_init_globals(
            globals.as_ptr().byte_add(1),
            globals.as_ptr().byte_add(1 + mem::size_of::<Global>()),
        );
        assert_eq!(*LEFT.0.get(), [5; 128]);
        assert_eq!(*RIGHT.0.get(), [5; 128]);
    }
}
