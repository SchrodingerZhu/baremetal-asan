//! Mapping application memory to logical and physical shadow ranges.
//!
//! Outlined checks take application addresses; shadow setters take the logical
//! address `(application >> SHADOW_SCALE) + SHADOW_OFFSET`. A layout maps these
//! onto borrowed slices, each contained in one physical backing region.

use core::{ops::Range, slice};

/// A piece of shadow and the application bytes it describes.
#[derive(Debug, PartialEq, Eq)]
pub struct Shadow<T> {
    pub memory: Range<usize>,
    pub bytes: T,
}

/// A contiguous application range whose shadow is stored across RAM regions.
///
/// Application and region bounds must be granule-aligned. Regions must cover the
/// application range in order, without overlaps or holes. Each shadow range must be
/// nonempty, disjoint from the others, and sized for its application region.
pub trait Layout: Sized {
    const APPLICATION: Range<usize>;
    const SHADOW_SCALE: u32;

    const GRANULE: usize = 1 << Self::SHADOW_SCALE;
    const SHADOW_SIZE: usize = (Self::APPLICATION.end - Self::APPLICATION.start) / Self::GRANULE;
    /// LLVM's linear shadow address is a logical address, translated by setters.
    const SHADOW_BASE: usize;
    /// LLVM adds this offset with pointer-width wrapping arithmetic.
    const SHADOW_OFFSET: usize =
        Self::SHADOW_BASE.wrapping_sub(Self::APPLICATION.start >> Self::SHADOW_SCALE);

    /// Split an application access into ranges backed by individual shadow regions.
    /// Empty, wrapping, and unsupported ranges yield no pieces; overlaps are clipped.
    /// The default maps to contiguous shadow starting at `SHADOW_BASE`.
    #[inline(always)]
    fn to_ranges(addr: usize, size: usize) -> impl Iterator<Item = Shadow<Range<usize>>> {
        let last = size.checked_sub(1).and_then(|size| addr.checked_add(size));
        map_region::<Self>(addr, last, Self::APPLICATION, Self::SHADOW_BASE).into_iter()
    }

    /// Borrow shadow in region-sized pieces without copying bytes.
    ///
    /// # Safety
    /// The layout must describe initialized, readable shadow RAM that remains
    /// unmodified while any returned slice is borrowed.
    #[inline(always)]
    unsafe fn to_slices(addr: usize, size: usize) -> impl Iterator<Item = Shadow<&'static [i8]>> {
        Self::to_ranges(addr, size).map(|part| Shadow {
            memory: part.memory,
            // SAFETY: each range lies in one backing region supplied by the caller.
            bytes: unsafe {
                slice::from_raw_parts(part.bytes.start as *const i8, part.bytes.len())
            },
        })
    }

    /// Mutably borrow shadow in region-sized pieces without copying bytes.
    ///
    /// # Safety
    /// The layout must describe initialized, writable shadow RAM. Each returned
    /// slice must have exclusive access to its bytes for the duration of its borrow.
    #[inline(always)]
    unsafe fn to_slices_mut(
        addr: usize,
        size: usize,
    ) -> impl Iterator<Item = Shadow<&'static mut [i8]>> {
        Self::to_ranges(addr, size).map(|part| Shadow {
            memory: part.memory,
            // SAFETY: the caller guarantees exclusive access to disjoint regions.
            bytes: unsafe {
                slice::from_raw_parts_mut(part.bytes.start as *mut i8, part.bytes.len())
            },
        })
    }

    /// Fill a compiler-provided logical shadow range, ignoring invalid ranges.
    ///
    /// # Safety
    /// The same initialization and exclusive-access requirements as `to_slices_mut`.
    #[inline(always)]
    unsafe fn set_shadow(addr: usize, size: usize, value: i8) {
        let Some(offset) = addr.checked_sub(Self::SHADOW_BASE) else {
            return;
        };
        // Validate the whole request before writing either backing region.
        if size == 0 || offset >= Self::SHADOW_SIZE || size > Self::SHADOW_SIZE - offset {
            return;
        }
        let memory = Self::APPLICATION.start + offset * Self::GRANULE;
        // SAFETY: the caller provides exclusive shadow access; the range is valid.
        unsafe { Self::to_slices_mut(memory, size * Self::GRANULE) }
            .for_each(|part| part.bytes.fill(value));
    }
}

/// Clip an access to one application region and map its covering granules.
#[inline(always)]
pub fn map_region<L: Layout>(
    addr: usize,
    last: Option<usize>,
    memory: Range<usize>,
    shadow: usize,
) -> Option<Shadow<Range<usize>>> {
    let first = addr.max(memory.start);
    let last = last?.min(memory.end - 1);
    (first <= last).then(|| Shadow {
        memory: first..last + 1,
        bytes: shadow + (first - memory.start) / L::GRANULE
            ..shadow + (last - memory.start) / L::GRANULE + 1,
    })
}
