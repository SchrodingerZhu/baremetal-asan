//! Shadow updates for granule-aligned heap and fake-stack allocations.

use crate::platform::Platform;

// Callers hold the heap critical section, own these application granules, and
// supply initialized shadow. Allocation bases and poisoned extents are aligned.
pub(crate) unsafe fn poison<P: Platform>(addr: usize, size: usize, value: u8) {
    unsafe { P::to_shadow_slices_mut(addr, size) }.for_each(|part| part.bytes.fill(value as i8));
}

// Make exactly `size` bytes at an aligned address accessible, including a final
// partial granule. The remainder of the allocation has already been poisoned.
pub(crate) unsafe fn unpoison<P: Platform>(addr: usize, size: usize) {
    let end = addr + size;
    let tail = size % P::GRANULE;
    unsafe { P::to_shadow_slices_mut(addr, size) }.for_each(|part| {
        part.bytes.fill(0);
        if tail != 0 && part.memory.end == end {
            *part.bytes.last_mut().unwrap() = tail as i8;
        }
    });
}
