//! RA8M2 memory layouts for the RA8x2 target runtime.
//!
//! ```text
//! Eight-byte granules, at the DTCM/SRAM split:
//!
//! ┌─────────────────────────┐   ┌─────────────────────────┐   ┌─────────────────────────┐
//! │   Application address   │   │  Logical shadow address │   │ Physical shadow address │
//! ├─────────────────────────┤   ├─────────────────────────┤   ├─────────────────────────┤
//! │       0x220f_fff8       │──▶│       0x2001_ffff       │──▶│    0x2001_ffff (DTCM)   │
//! │       0x2210_0000       │──▶│       0x2002_0000       │──▶│    0x2218_c000 (SRAM)   │
//! └─────────────────────────┘   └─────────────────────────┘   └─────────────────────────┘
//! ```
//!
//! Outlined access checks receive application addresses; LLVM's shadow setters
//! receive logical shadow addresses. Both use the same physical backing slices.
//! With sixteen-byte granules, logical and physical shadow coincide in DTCM.

use baremetal_asan_rt::layout::{Layout, Shadow, map_region};
use core::ops::Range;

/// Eight-byte granules: DTCM first, then the reserved tail of main SRAM.
pub struct Ra8m2Granule8;

impl Layout for Ra8m2Granule8 {
    // Reserve the last 80 KiB of main SRAM for shadow (70 KiB is used).
    const APPLICATION: Range<usize> = 0x2200_0000..0x2218_c000;
    const SHADOW_SCALE: u32 = 3;
    const SHADOW_BASE: usize = 0x2000_0000;

    #[inline(always)]
    fn to_ranges(addr: usize, size: usize) -> impl Iterator<Item = Shadow<Range<usize>>> {
        let last = size.checked_sub(1).and_then(|size| addr.checked_add(size));
        // Form each optional piece before chaining to keep the fixed region count
        // visible to the consumer, without iterating a region table.
        map_region::<Self>(addr, last, 0x2200_0000..0x2210_0000, 0x2000_0000)
            .into_iter()
            .chain(map_region::<Self>(
                addr,
                last,
                0x2210_0000..0x2218_c000,
                0x2218_c000,
            ))
    }
}

/// Sixteen-byte granules: all main SRAM is covered by shadow in DTCM.
pub struct Ra8m2Granule16;

impl Layout for Ra8m2Granule16 {
    const APPLICATION: Range<usize> = 0x2200_0000..0x221a_0000;
    const SHADOW_SCALE: u32 = 4;
    const SHADOW_BASE: usize = 0x2000_0000;
}

/// Eight-byte granules with shadow entirely in main SRAM; DTCM is unused.
pub struct Ra8m2Granule8Sram;

impl Layout for Ra8m2Granule8Sram {
    // Reserve the final 192 KiB of main SRAM; 184 KiB holds application shadow.
    const APPLICATION: Range<usize> = 0x2200_0000..0x2217_0000;
    const SHADOW_SCALE: u32 = 3;
    const SHADOW_BASE: usize = 0x2217_0000;
}

#[cfg(all(feature = "granule-16", feature = "no-dtcm"))]
compile_error!("granule-16 and no-dtcm select different layouts; enable only one");

#[cfg(all(not(feature = "granule-16"), not(feature = "no-dtcm")))]
pub type ActiveLayout = Ra8m2Granule8;
#[cfg(feature = "granule-16")]
pub type ActiveLayout = Ra8m2Granule16;
#[cfg(all(feature = "no-dtcm", not(feature = "granule-16")))]
pub type ActiveLayout = Ra8m2Granule8Sram;

#[cfg(test)]
mod tests;
