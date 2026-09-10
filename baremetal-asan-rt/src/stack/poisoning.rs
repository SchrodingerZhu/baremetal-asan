//! Stack lifetime and dynamic-allocation shadow updates, matching compiler-rt.

use crate::platform::Platform;

unsafe fn set_stack_memory<P: Platform, const POISON: bool>(addr: usize, size: usize) {
    let Some(end) = addr.checked_add(size) else {
        return;
    };
    let tail = (size % P::GRANULE) as i8;
    // SAFETY: the caller owns the stack object's initialized shadow. The platform
    // splits at physical region boundaries; only the final piece can have a tail.
    unsafe { P::to_shadow_slices_mut(addr, size) }.for_each(|part| {
        let whole = if tail != 0 && part.memory.end == end {
            let (last, whole) = part.bytes.split_last_mut().unwrap();
            if POISON {
                // A shadow byte cannot describe a poisoned prefix followed by
                // live bytes. Preserve any live suffix outside this lifetime.
                if *last > 0 && *last <= tail {
                    *last = 0xf8u8 as i8;
                }
            } else if *last != 0 {
                // Extend the live prefix without shortening an existing one.
                *last = (*last).max(tail);
            }
            whole
        } else {
            part.bytes
        };
        whole.fill(if POISON { 0xf8u8 as i8 } else { 0 });
    });
}

/// Mark a stack object's ended lifetime with use-after-scope poison (`0xf8`).
/// A partial final granule preserves any live suffix outside the object.
///
/// # Safety
/// `addr` must be granule-aligned. The caller must exclusively own the object's
/// initialized shadow for this update, including its final partial granule.
pub unsafe fn poison_stack_memory<P: Platform>(addr: usize, size: usize) {
    unsafe { set_stack_memory::<P, true>(addr, size) };
}

/// Make a stack object accessible at lifetime start, preserving live neighbors.
///
/// # Safety
/// The alignment and shadow ownership requirements of `poison_stack_memory` apply.
pub unsafe fn unpoison_stack_memory<P: Platform>(addr: usize, size: usize) {
    unsafe { set_stack_memory::<P, false>(addr, size) };
}

/// Set LLVM's 32-byte dynamic-alloca redzones (`0xca` left, `0xcb` right).
/// Right padding extends to a 32-byte boundary; a partial payload granule keeps
/// exactly its valid prefix accessible. Complete payload granules are unchanged.
///
/// # Safety
/// LLVM must have reserved the redzones around the 32-byte-aligned `addr` and
/// initialized the payload's shadow. The caller exclusively owns all affected
/// shadow, and the platform granule must divide 32.
pub unsafe fn alloca_poison<P: Platform>(addr: usize, size: usize) {
    const REDZONE: usize = 32;
    const { assert!(32 % P::GRANULE == 0) };
    let Some(left) = addr.checked_sub(REDZONE) else {
        return;
    };
    let Some(end) = addr.checked_add(size) else {
        return;
    };
    let Some(right_end) = end
        .checked_add(REDZONE - 1)
        .map(|end| end & !(REDZONE - 1))
        .and_then(|end| end.checked_add(REDZONE))
    else {
        return;
    };
    let partial = end & !(P::GRANULE - 1);
    // SAFETY: the caller reserves the compiler's redzones and their shadow.
    unsafe { P::to_shadow_slices_mut(left, REDZONE) }
        .for_each(|part| part.bytes.fill(0xcau8 as i8));
    unsafe { P::to_shadow_slices_mut(partial, right_end - partial) }.for_each(|part| {
        part.bytes.fill(0xcbu8 as i8);
        if end != partial && part.memory.start == partial {
            part.bytes[0] = (end - partial) as i8;
        }
    });
}

/// Clear dynamic-stack shadow before return or stack restore.
/// LLVM's `top` is the lower address; `bottom` is the saved upper bound. Clear
/// only complete granules, leaving the shadow at an unaligned upper bound alone.
///
/// # Safety
/// `top` must be granule-aligned, or zero to skip cleanup. The caller must own
/// the initialized shadow for this retired dynamic-stack range exclusively.
pub unsafe fn allocas_unpoison<P: Platform>(top: usize, bottom: usize) {
    if top == 0 {
        return;
    }
    let Some(size) = bottom.checked_sub(top) else {
        return;
    };
    // SAFETY: the caller supplies the retired range and exclusive shadow access.
    unsafe { P::to_shadow_slices_mut(top, size & !(P::GRANULE - 1)) }
        .for_each(|part| part.bytes.fill(0));
}
