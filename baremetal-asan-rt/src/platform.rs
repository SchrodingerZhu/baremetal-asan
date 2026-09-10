//! Platform memory: application, shadow, stack bounds, and allocation source.
//!
//! - **Application address**: the address of the program byte being checked.
//!   Each `GRANULE`-byte application block is described by one shadow byte.
//! - **Logical shadow address**: LLVM's linear mapping
//!   `(addr >> SHADOW_SCALE) + SHADOW_OFFSET`. It starts at `SHADOW_BASE` for
//!   `APPLICATION.start` and advances by one per granule. This address need not
//!   refer to accessible RAM; it identifies a byte in the logical shadow space.
//! - **Physical shadow address**: the actual RAM address holding that shadow byte.
//!   The platform places consecutive logical shadow bytes in one or more backing
//!   regions, which may be separated by address gaps.
//!
//! ```text
//! ┌─────────────────────────┐   ┌─────────────────────────┐   ┌─────────────────────────┐
//! │   Application address   │──▶│  Logical shadow address │──▶│ Physical shadow address │
//! └─────────────────────────┘   └─────────────────────────┘   └─────────────────────────┘
//!
//! Logical   ┌───────────────┬───────────────┐
//!           │  first part   │  second part  │
//!           └───────┬───────┴───────┬───────┘
//!                   │               └─────────────┐
//!                   ▼                             ▼
//! Physical  ┌───────────────┐             ┌───────────────┐
//!           │   region A    │ address gap │   region B    │
//!           └───────────────┘             └───────────────┘
//! ```
//!
//! Outlined access checks receive application addresses and use `to_shadow_slices` to
//! borrow the corresponding physical shadow. Each slice stays within one region.
//! Shadow setters instead receive logical shadow addresses and shadow-byte counts.
//! `set_shadow` subtracts `SHADOW_BASE`, recovers the application granules, and
//! uses `to_shadow_slices_mut` to fill their physical shadow pieces.
//!
//! The default platform mapping makes logical and physical shadow addresses equal.
//! When they differ, LLVM's shadow operations must be outlined so the runtime can
//! perform the translation; a direct LLVM shadow access would bypass it.

use core::{ops::Range, slice};

/// A piece of shadow and the application bytes it describes.
#[derive(Debug, PartialEq, Eq)]
pub struct Shadow<T> {
    /// Application bytes described by this piece of shadow.
    pub memory: Range<usize>,
    /// The physical shadow address range or borrowed shadow bytes.
    pub bytes: T,
}

/// Runtime memory configuration and target hooks.
///
/// Application and region bounds must be granule-aligned. Regions must cover the
/// application range in order, without overlaps or holes. Each shadow range must be
/// nonempty, disjoint from the others, and sized for its application region.
pub trait Platform: Sized {
    const APPLICATION: Range<usize>;
    const SHADOW_SCALE: u32;

    const GRANULE: usize = 1 << Self::SHADOW_SCALE;
    const SHADOW_SIZE: usize = (Self::APPLICATION.end - Self::APPLICATION.start) / Self::GRANULE;
    /// Logical shadow address corresponding to `APPLICATION.start`.
    const SHADOW_BASE: usize;
    /// LLVM adds this offset with pointer-width wrapping arithmetic.
    const SHADOW_OFFSET: usize =
        Self::SHADOW_BASE.wrapping_sub(Self::APPLICATION.start >> Self::SHADOW_SCALE);

    /// Exclusive top of the current downward-growing stack; zero skips cleanup.
    #[inline(always)]
    fn stack_top() -> usize {
        0
    }

    /// Base application address of the arena reserved for heap and fake-stack
    /// allocations. Its `alloc_size()` bytes must be writable, within APPLICATION,
    /// and disjoint from program data, the real stack, and physical shadow.
    #[inline(always)]
    fn alloc_base() -> usize {
        0
    }

    /// Allocation arena size in bytes; zero means no allocation source.
    /// The base and size must remain fixed while the runtime uses the arena.
    #[inline(always)]
    fn alloc_size() -> usize {
        0
    }

    /// Split an application access into ranges backed by individual shadow regions.
    /// Empty, wrapping, and unsupported ranges yield no pieces; overlaps are clipped.
    /// The default maps to contiguous shadow starting at `SHADOW_BASE`.
    #[inline(always)]
    fn to_shadow_ranges(addr: usize, size: usize) -> impl Iterator<Item = Shadow<Range<usize>>> {
        let last = size.checked_sub(1).and_then(|size| addr.checked_add(size));
        map_region::<Self>(addr, last, Self::APPLICATION, Self::SHADOW_BASE).into_iter()
    }

    /// Borrow shadow in region-sized pieces without copying bytes.
    ///
    /// # Safety
    /// The platform must describe initialized, readable shadow RAM that remains
    /// unmodified while any returned slice is borrowed.
    #[inline(always)]
    unsafe fn to_shadow_slices(
        addr: usize,
        size: usize,
    ) -> impl Iterator<Item = Shadow<&'static [i8]>> {
        Self::to_shadow_ranges(addr, size).map(|part| Shadow {
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
    /// The platform must describe initialized, writable shadow RAM. Each returned
    /// slice must have exclusive access to its bytes for the duration of its borrow.
    #[inline(always)]
    unsafe fn to_shadow_slices_mut(
        addr: usize,
        size: usize,
    ) -> impl Iterator<Item = Shadow<&'static mut [i8]>> {
        Self::to_shadow_ranges(addr, size).map(|part| Shadow {
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
    /// The same initialization and exclusive-access requirements as `to_shadow_slices_mut`.
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
        unsafe { Self::to_shadow_slices_mut(memory, size * Self::GRANULE) }
            .for_each(|part| part.bytes.fill(value));
    }
}

/// Clip an access to one application region and map its covering granules.
#[inline(always)]
pub fn map_region<P: Platform>(
    addr: usize,
    last: Option<usize>,
    memory: Range<usize>,
    shadow: usize,
) -> Option<Shadow<Range<usize>>> {
    let first = addr.max(memory.start);
    let last = last?.min(memory.end - 1);
    (first <= last).then(|| Shadow {
        memory: first..last + 1,
        bytes: shadow + (first - memory.start) / P::GRANULE
            ..shadow + (last - memory.start) / P::GRANULE + 1,
    })
}
