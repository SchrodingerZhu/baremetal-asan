extern crate std;

use super::*;
use crate::layout::{Shadow, map_region};
use core::ops::Range;
use std::{format, string::String, vec::Vec};

// Split a row after its ninth byte to exercise a physical-region boundary that
// does not coincide with a display-row boundary. All backing bytes are immutable.
static LEFT: [i8; 9] = [-15; 9];
static RIGHT: [i8; 26] = [-13; 26];

struct Split<const SCALE: u32>;
impl<const SCALE: u32> Layout for Split<SCALE> {
    const APPLICATION: Range<usize> = 0x1000..0x1000 + 35 * Self::GRANULE;
    const SHADOW_SCALE: u32 = SCALE;
    const SHADOW_BASE: usize = 0;

    fn to_ranges(addr: usize, size: usize) -> impl Iterator<Item = Shadow<Range<usize>>> {
        let last = size.checked_sub(1).and_then(|n| addr.checked_add(n));
        let split = Self::APPLICATION.start + LEFT.len() * Self::GRANULE;
        map_region::<Self>(
            addr,
            last,
            Self::APPLICATION.start..split,
            LEFT.as_ptr() as usize,
        )
        .into_iter()
        .chain(map_region::<Self>(
            addr,
            last,
            split..Self::APPLICATION.end,
            RIGHT.as_ptr() as usize,
        ))
    }
}

fn dump<L: Layout>(addr: usize) -> String {
    // Only used with immutable Split backing or addresses outside a layout.
    format!("{}", ShadowDump::<L>(addr, PhantomData))
}

#[test]
fn colors_and_marks_the_correct_byte_across_a_split_row() {
    let text = dump::<Split<3>>(0x104b);
    assert!(text.contains("[\x1b[1;31mf3\x1b[0m]"));
    let row = text.lines().find(|line| line.starts_with("=>")).unwrap();
    let plain = row.replace("\x1b[1;31m", "").replace("\x1b[0m", "");
    assert_eq!(
        plain,
        "=>0x00001000: f1 f1 f1 f1 f1 f1 f1 f1 f1 [f3] f3 f3 f3 f3 f3 f3"
    );
}

fn check_bounds<L: Layout>() {
    for addr in [L::APPLICATION.start, L::APPLICATION.end - 1] {
        let text = dump::<L>(addr);
        let rows: Vec<_> = text
            .lines()
            .filter(|line| line.starts_with("  0x") || line.starts_with("=>"))
            .collect();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].split_whitespace().count(), 17);
        assert_eq!(rows[1].split_whitespace().count(), 17);
        assert_eq!(rows[2].split_whitespace().count(), 4);
        assert_eq!(rows.iter().filter(|row| row.starts_with("=>")).count(), 1);
    }
}

#[test]
fn clips_both_application_boundaries_for_both_granules() {
    check_bounds::<Split<3>>();
    check_bounds::<Split<4>>();
    let text = dump::<Split<4>>(Split::<4>::APPLICATION.end - 1);
    assert!(text.contains("=>0x00001200:"));
    assert!(text.contains("one shadow byte represents 16 application bytes"));
    assert!(text.contains(" 08 09 0a 0b 0c 0d 0e 0f\n"));
}

#[test]
fn unsupported_addresses_do_not_read_unmapped_shadow() {
    use crate::test_layout::Granule8;
    for addr in [
        0,
        Granule8::APPLICATION.start - 1,
        Granule8::APPLICATION.end,
        usize::MAX,
    ] {
        assert!(dump::<Granule8>(addr).is_empty());
    }
}
