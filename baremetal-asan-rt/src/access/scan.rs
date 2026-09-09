//! Scanning borrowed shadow slices and fixed arrays without copying them.

use crate::layout::Layout;

pub(super) trait ShadowScan {
    /// Return the first invalid application address in `first..=last`.
    /// The shadow contains exactly the granules covering this validated range.
    fn find_invalid_shadow_byte<L: Layout>(self, first: usize, last: usize) -> Option<usize>;
}

impl ShadowScan for &[i8] {
    #[inline(always)]
    fn find_invalid_shadow_byte<L: Layout>(self, first: usize, last: usize) -> Option<usize> {
        #[cfg(all(feature = "mve", target_arch = "arm", target_os = "none"))]
        // MVE uses zero as its success sentinel and compares signed shadow bytes
        // against the granule size. Other layouts retain the scalar scan.
        if L::APPLICATION.start != 0 && L::GRANULE < 128 && self.len() >= 16 && !mve::in_handler() {
            return mve::find_invalid_shadow_byte::<L>(self, first, last);
        }

        let base = first - first % L::GRANULE;
        self.iter().enumerate().find_map(|(index, &value)| {
            let granule = base + index * L::GRANULE;
            poisoned_in_granule::<L>(value, granule, first.max(granule), last)
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
            fn find_invalid_shadow_byte<L: Layout>(self, first: usize, last: usize) -> Option<usize> {
                let base = first - first % L::GRANULE;
                None$(.or_else(|| {
                    let granule = base + $index * L::GRANULE;
                    poisoned_in_granule::<L>(self[$index], granule, first.max(granule), last)
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
/// `granule` is the aligned start of an application block of `L::GRANULE` bytes.
/// `first` must lie inside that block; `last` is inclusive and may extend
/// beyond it, so the check clips `last` to the block's end.
///
/// Shadow 0 allows the whole block; a negative value poisons it entirely.
/// A positive value allows that many leading bytes of the block.
#[inline(always)]
fn poisoned_in_granule<L: Layout>(
    shadow: i8,
    granule: usize,
    first: usize,
    last: usize,
) -> Option<usize> {
    if shadow == 0 {
        None
    } else if shadow < 0 {
        Some(first)
    } else if last.min(granule + L::GRANULE - 1) >= granule + shadow as usize {
        Some(first.max(granule + shadow as usize))
    } else {
        None
    }
}

#[cfg(all(feature = "mve", target_arch = "arm", target_os = "none"))]
mod mve {
    //! Exact MVE slice scan, outlined so handlers never enter vector code under LTO.

    use crate::layout::Layout;
    use core::arch::asm;

    #[inline(always)]
    pub(super) fn in_handler() -> bool {
        let ipsr: u32;
        // Reading IPSR uses a core register and cannot trigger lazy FP preservation.
        unsafe { asm!("mrs {}, IPSR", out(reg) ipsr, options(nomem, nostack, preserves_flags)) };
        ipsr != 0
    }

    /// Scan the shadow of a validated, nonempty application-SRAM range.
    /// Startup must enable MVE and set FPSCR.LEN to 0b100 (no tail-predicated loop).
    #[inline(never)]
    pub(super) fn find_invalid_shadow_byte<L: Layout>(
        shadow: &[i8],
        first: usize,
        last: usize,
    ) -> Option<usize> {
        let invalid: usize;
        // SAFETY: the slice covers exactly first..=last. Predicated loads zero
        // inactive lanes without reading outside it. Only a selected active lane
        // is reread to recover its poison value. All vector clobbers are declared;
        // VPR is preserved explicitly because Rust has no VPR clobber operand.
        unsafe {
            asm!(
                "vmrs {saved}, vpr",
                "vmov.i8 q1, #{granule}",
                "2:",
                "vctp.8 {remaining}",
                "vpst",
                "vldrbt.u8 q0, [{shadow}]",
                // A granule contains poison iff its signed shadow is nonzero and
                // below the granule size. VPT + VCMPT intersects those predicates in P0.
                "vpt.i8 ne, q0, zr",
                "vcmpt.s8 lt, q0, q1",
                "vmrs {invalid}, p0",
                "cmp {invalid}, #0",
                "bne 3f",
                "subs {remaining}, {remaining}, #16",
                "bls 4f",
                "adds {shadow}, {shadow}, #16",
                "adds {base}, {base}, #{advance}",
                "b 2b",
                "3:",
                // P0 has one bit per byte. CTZ locates the first poisoned granule;
                // negative shadow starts poison at offset 0, positive at its value.
                "rbit {invalid}, {invalid}",
                "clz {invalid}, {invalid}",
                "ldrsb {value}, [{shadow}, {invalid}]",
                "cmp {value}, #0",
                "it lt",
                "movlt {value}, #0",
                "add {base}, {base}, {invalid}, lsl #{scale}",
                "add {invalid}, {base}, {value}",
                "cmp {invalid}, {first}",
                "csel {invalid}, {invalid}, {first}, hs",
                // Poison beyond the last accessed byte is a valid partial tail.
                "cmp {invalid}, {last}",
                "it hi",
                "movhi {invalid}, #0",
                "4:",
                "vmsr vpr, {saved}",
                shadow = inout(reg) shadow.as_ptr() => _,
                remaining = inout(reg) shadow.len() => _,
                base = inout(reg) first & !(L::GRANULE - 1) => _,
                granule = const L::GRANULE,
                advance = const 16 * L::GRANULE,
                scale = const L::SHADOW_SCALE,
                first = in(reg) first,
                last = in(reg) last,
                invalid = out(reg) invalid,
                value = out(reg) _,
                saved = out(reg) _,
                out("q0") _,
                out("q1") _,
                options(nostack, readonly),
            );
        }
        // The MVE dispatch excludes layouts containing address zero.
        if invalid == 0 { None } else { Some(invalid) }
    }
}
