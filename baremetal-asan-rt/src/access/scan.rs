//! Scanning borrowed shadow slices and fixed arrays without copying them.

use super::ASAN_QUANTUM;

pub(super) trait ShadowScan {
    /// Return the first invalid application address in `first..=last`.
    /// The shadow contains exactly the granules covering this validated range.
    fn find_invalid_shadow_byte(self, first: usize, last: usize) -> Option<usize>;
}

impl ShadowScan for &[i8] {
    #[inline(always)]
    fn find_invalid_shadow_byte(self, first: usize, last: usize) -> Option<usize> {
        let base = first - first % ASAN_QUANTUM;
        self.iter().enumerate().find_map(|(index, &value)| {
            let granule = base + index * ASAN_QUANTUM;
            poisoned_in_granule(value, granule, first.max(granule), last)
        })
    }
}

// Ordinary borrowed-array iteration becomes a slice iterator. Expand the fixed
// cases here so even -Os gets straight-line checks, independent of loop unrolling.
// or_else keeps later shadow reads lazy and preserves the first-match order.
macro_rules! fixed_shadow_scan {
    ($size:literal; $($index:literal),+) => {
        impl ShadowScan for &[i8; $size] {
            #[inline(always)]
            fn find_invalid_shadow_byte(self, first: usize, last: usize) -> Option<usize> {
                let base = first - first % ASAN_QUANTUM;
                None$(.or_else(|| {
                    let granule = base + $index * ASAN_QUANTUM;
                    poisoned_in_granule(self[$index], granule, first.max(granule), last)
                }))+
            }
        }
    };
}

fixed_shadow_scan!(1; 0);
fixed_shadow_scan!(2; 0, 1);
fixed_shadow_scan!(3; 0, 1, 2);

/// Check `first..=last` against one shadow byte, returning the first invalid
/// application address or `None`.
///
/// `granule` is the aligned start of an 8-byte application-memory block.
/// `first` must lie inside that block; `last` is inclusive and may extend
/// beyond it, so the check clips `last` to the block's end.
///
/// Shadow 0 allows the whole block; a negative value poisons it entirely.
/// A positive value allows that many leading bytes of the block.
#[inline(always)]
fn poisoned_in_granule(shadow: i8, granule: usize, first: usize, last: usize) -> Option<usize> {
    if shadow == 0 {
        None
    } else if shadow < 0 {
        Some(first)
    } else if last.min(granule + ASAN_QUANTUM - 1) >= granule + shadow as usize {
        Some(first.max(granule + shadow as usize))
    } else {
        None
    }
}
