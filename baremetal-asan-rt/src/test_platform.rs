use crate::platform::Platform;
use core::ops::Range;

pub struct TestPlatform<const SCALE: u32>;
impl<const SCALE: u32> Platform for TestPlatform<SCALE> {
    const APPLICATION: Range<usize> = 0x0010_0000..0x0020_0000;
    const SHADOW_SCALE: u32 = SCALE;
    const SHADOW_BASE: usize = 0x0030_0000;
}

pub type Granule8 = TestPlatform<3>;
pub type Granule16 = TestPlatform<4>;

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
