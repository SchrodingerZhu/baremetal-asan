//! Scanning borrowed shadow slices and fixed arrays without copying them.

pub(super) trait ShadowScan {
    /// Return the first match, passing each shadow byte's index and signed value.
    fn find_map<R>(self, f: impl FnMut(usize, i8) -> Option<R>) -> Option<R>;
}

impl ShadowScan for &[i8] {
    #[inline(always)]
    fn find_map<R>(self, mut f: impl FnMut(usize, i8) -> Option<R>) -> Option<R> {
        self.iter()
            .enumerate()
            .find_map(|(index, &value)| f(index, value))
    }
}

// Ordinary borrowed-array iteration becomes a slice iterator. Expand the fixed
// cases here so even -Os gets straight-line checks, independent of loop unrolling.
// or_else keeps later shadow reads lazy and preserves the first-match order.
macro_rules! fixed_shadow_scan {
    ($size:literal; $($index:literal),+) => {
        impl ShadowScan for &[i8; $size] {
            #[inline(always)]
            fn find_map<R>(self, mut f: impl FnMut(usize, i8) -> Option<R>) -> Option<R> {
                None$(.or_else(|| f($index, self[$index])))+
            }
        }
    };
}

fixed_shadow_scan!(1; 0);
fixed_shadow_scan!(2; 0, 1);
fixed_shadow_scan!(3; 0, 1, 2);
