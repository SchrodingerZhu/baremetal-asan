//! ASan runtime diagnostics.

use crate::platform::Platform;
use core::{fmt, marker::PhantomData};

/// # Safety
/// The platform's shadow must remain initialized, readable, and unchanged while
/// the panic handler formats the diagnostic, as required by `Platform::to_slices`.
#[cold]
#[inline(never)]
pub(crate) unsafe fn report_access<P: Platform>(
    addr: usize,
    size: usize,
    is_write: bool,
    invalid: usize,
) -> ! {
    panic!(
        "ASan: {} of {} byte(s) at {:#x}; first invalid address {:#x}\n{}",
        if is_write { "store" } else { "load" },
        size,
        addr,
        invalid,
        ShadowDump::<P>(invalid, PhantomData),
    );
}

// Constructed only under report_access's shadow-access contract. Formatting via
// the panic message keeps the portable runtime independent of its output device.
struct ShadowDump<P>(usize, PhantomData<P>);

impl<P: Platform> fmt::Display for ShadowDump<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !P::APPLICATION.contains(&self.0) {
            return Ok(());
        }

        const COLUMNS: usize = 16;
        let guilty = (self.0 - P::APPLICATION.start) / P::GRANULE;
        let center = guilty / COLUMNS * COLUMNS;
        let first = center.saturating_sub(5 * COLUMNS);
        let end = center.saturating_add(6 * COLUMNS).min(P::SHADOW_SIZE);
        writeln!(
            f,
            "Shadow bytes around the buggy address (application addresses):"
        )?;
        for row in (first..end).step_by(COLUMNS) {
            let addr = P::APPLICATION.start + row * P::GRANULE;
            let count = COLUMNS.min(end - row);
            let prefix = if row == center { "=>" } else { "  " };
            write!(f, "{prefix}{addr:#010x}:")?;
            // SAFETY: construction guarantees readable, stable shadow. The
            // window is clipped to APPLICATION; each slice stays in one region.
            let pieces = unsafe { P::to_slices(addr, count * P::GRANULE) };
            for (column, &value) in pieces.flat_map(|part| part.bytes).enumerate() {
                let byte = ShadowByte(value as u8);
                if row + column == guilty {
                    write!(f, " [{byte}]")?;
                } else {
                    write!(f, " {byte}")?;
                }
            }
            writeln!(f)?;
        }

        writeln!(
            f,
            "Shadow byte legend (one shadow byte represents {} application bytes):",
            P::GRANULE,
        )?;
        writeln!(f, "  Addressable:           {}", ShadowByte(0))?;
        write!(f, "  Partially addressable:")?;
        for value in 1..P::GRANULE {
            write!(f, " {}", ShadowByte(value as u8))?;
        }
        writeln!(f)?;
        for (name, value) in [
            ("Heap left redzone", 0xfa),
            ("Freed heap region", 0xfd),
            ("Stack left redzone", 0xf1),
            ("Stack mid redzone", 0xf2),
            ("Stack right redzone", 0xf3),
            ("Stack after return", 0xf5),
            ("Stack use after scope", 0xf8),
            ("Global redzone", 0xf9),
            ("Global init order", 0xf6),
            ("Poisoned by user", 0xf7),
            ("Container overflow", 0xfc),
            ("Array cookie", 0xac),
            ("Intra object redzone", 0xbb),
            ("ASan internal", 0xfe),
            ("Left alloca redzone", 0xca),
            ("Right alloca redzone", 0xcb),
        ] {
            writeln!(f, "  {name}: {}", ShadowByte(value))?;
        }
        Ok(())
    }
}

struct ShadowByte(u8);

impl fmt::Display for ShadowByte {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Match compiler-rt's poison categories. ANSI escapes pass through the
        // target's panic output and are interpreted by the host terminal.
        let color = match self.0 {
            0xfa | 0xac | 0xf1..=0xf3 | 0xf9 => "\x1b[1;31m",
            0xfd | 0xf5 | 0xf8 => "\x1b[1;35m",
            0xf6 => "\x1b[1;36m",
            0xf7 | 0xfc | 0xca | 0xcb => "\x1b[1;34m",
            0xfe | 0xbb => "\x1b[1;33m",
            _ => return write!(f, "{:02x}", self.0),
        };
        write!(f, "{color}{:02x}\x1b[0m", self.0)
    }
}

#[cold]
#[inline(never)]
pub(crate) fn report_memcpy_overlap(dst: usize, src: usize, size: usize) -> ! {
    panic!("ASan: memcpy of {size} byte(s) has overlapping ranges at {dst:#x} and {src:#x}");
}

#[cfg(test)]
mod tests;
