extern crate std;

use crate::platform::{Platform, Shadow};
use core::ops::Range;

pub struct TestPlatform<const SCALE: u32>;
impl<const SCALE: u32> Platform for TestPlatform<SCALE> {
    const APPLICATION: Range<usize> = 0x0010_0000..0x0020_0000;
    const SHADOW_SCALE: u32 = SCALE;
    const SHADOW_BASE: usize = 0x0030_0000;
}

pub type Granule8 = TestPlatform<3>;
pub type Granule16 = TestPlatform<4>;

static ROM_SHADOW: [i8; 4] = [0, 1, 0xf9u8 as i8, 0xf9u8 as i8];

pub struct RomPlatform<const SCALE: u32, const START: usize>;
impl<const SCALE: u32, const START: usize> Platform for RomPlatform<SCALE, START> {
    const APPLICATION: Range<usize> = 0x1000..0x1040;
    const SHADOW_SCALE: u32 = SCALE;
    const SHADOW_BASE: usize = 0x3000;

    fn rom_shadow() -> Option<Shadow<Range<usize>>> {
        let base = ROM_SHADOW.as_ptr() as usize;
        Some(Shadow {
            memory: START..START + ROM_SHADOW.len() * Self::GRANULE,
            bytes: base..base + ROM_SHADOW.len(),
        })
    }
}

fn check_rom<P: Platform>() {
    let rom = P::rom_shadow().unwrap();
    let parts: std::vec::Vec<_> = P::to_shadow_ranges(0, usize::MAX).collect();
    assert_eq!(parts.len(), 2);
    assert!(parts[0].memory.end <= parts[1].memory.start);
    assert!(parts.contains(&rom));
    assert_eq!(
        P::to_shadow_ranges(rom.memory.start - 1, rom.memory.len() + 2).next(),
        Some(rom),
    );
    let rom = P::rom_shadow().unwrap();
    assert!(
        P::to_writable_shadow_ranges(rom.memory.start, rom.memory.len())
            .next()
            .is_none()
    );
    unsafe {
        let read = P::to_shadow_slices(rom.memory.start, rom.memory.len())
            .next()
            .unwrap();
        assert_eq!(read.bytes, ROM_SHADOW);
        assert_eq!(read.bytes.as_ptr(), ROM_SHADOW.as_ptr());
        assert!(
            P::to_shadow_slices_mut(rom.memory.start, rom.memory.len())
                .next()
                .is_none()
        );
        crate::stack::poison_stack_memory::<P>(rom.memory.start, rom.memory.len());
        crate::stack::unpoison_stack_memory::<P>(rom.memory.start, rom.memory.len());
        P::set_shadow(
            (rom.memory.start >> P::SHADOW_SCALE).wrapping_add(P::SHADOW_OFFSET),
            ROM_SHADOW.len(),
            0,
        );
        assert_eq!(read.bytes, ROM_SHADOW);
        crate::access::check_range::<P>(rom.memory.start, P::GRANULE + 1, false);
        for offset in [P::GRANULE + 1, 2 * P::GRANULE] {
            assert!(
                std::panic::catch_unwind(|| {
                    crate::access::check_range::<P>(rom.memory.start + offset, 1, false);
                })
                .is_err()
            );
        }
    }
}

#[test]
fn rom_reads_and_poisoning_are_separate_for_both_granules_and_address_orders() {
    check_rom::<RomPlatform<3, 0x800>>();
    check_rom::<RomPlatform<4, 0x800>>();
    check_rom::<RomPlatform<3, 0x2000>>();
    check_rom::<RomPlatform<4, 0x2000>>();
}

#[test]
fn ram_accesses_do_not_resolve_rom_bounds() {
    struct RamOnly;
    impl Platform for RamOnly {
        const APPLICATION: Range<usize> = 0x1000..0x1040;
        const SHADOW_SCALE: u32 = 3;
        const SHADOW_BASE: usize = 0x2000;

        fn rom_shadow() -> Option<Shadow<Range<usize>>> {
            panic!("RAM check queried ROM bounds");
        }
    }
    assert_eq!(
        RamOnly::to_shadow_ranges(0x1001, 8).next().unwrap().bytes,
        0x2000..0x2002
    );
}

#[test]
fn logical_mapping_supports_shadow_below_shifted_application_addresses() {
    struct LowShadow;
    impl Platform for LowShadow {
        const APPLICATION: Range<usize> = 0x8000_0000..0x8001_0000;
        const SHADOW_SCALE: u32 = 3;
        const SHADOW_BASE: usize = 0x2000;
    }
    for (app, shadow) in [(0x8000_0000usize, 0x2000), (0x8000_ffff, 0x3fff)] {
        assert_eq!(
            (app >> LowShadow::SHADOW_SCALE).wrapping_add(LowShadow::SHADOW_OFFSET),
            shadow
        );
        assert_eq!(
            LowShadow::to_shadow_ranges(app, 1).next().unwrap().bytes,
            shadow..shadow + 1
        );
    }
}
