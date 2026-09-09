use super::*;
use crate::test_layout::{Granule8, Granule16};

fn check_fixed_size<L: Layout, const SIZE: usize>() {
    let start = L::APPLICATION.start;
    for slot in 0..3 {
        for value in i8::MIN..=i8::MAX {
            let mut shadow = [0; 3];
            shadow[slot] = value;
            for offset in 0..L::GRANULE {
                // Independent reference: check every application byte.
                let expected = (offset..offset + SIZE)
                    .find(|&byte| {
                        let value = shadow[byte / L::GRANULE];
                        value < 0 || (value > 0 && byte % L::GRANULE >= value as usize)
                    })
                    .map(|byte| start + byte);
                let first = start + offset;
                let last = first + SIZE - 1;
                let count = (offset + SIZE).div_ceil(L::GRANULE);
                assert_eq!(
                    shadow[..count].find_invalid_shadow_byte::<L>(first, last),
                    expected,
                    "size={SIZE}, granule={}, shadow={shadow:?}, offset={offset}",
                    L::GRANULE,
                );
                let actual = match count {
                    1 => shadow
                        .first_chunk::<1>()
                        .unwrap()
                        .find_invalid_shadow_byte::<L>(first, last),
                    2 => shadow
                        .first_chunk::<2>()
                        .unwrap()
                        .find_invalid_shadow_byte::<L>(first, last),
                    3 => shadow.find_invalid_shadow_byte::<L>(first, last),
                    _ => unreachable!(),
                };
                assert_eq!(actual, expected, "fixed size={SIZE}, offset={offset}");
            }
        }
    }
}

fn check_fixed_sizes<L: Layout>() {
    check_fixed_size::<L, 1>();
    check_fixed_size::<L, 2>();
    check_fixed_size::<L, 4>();
    check_fixed_size::<L, 8>();
    check_fixed_size::<L, 16>();
}

#[test]
fn fixed_sizes_match_bytewise_checks() {
    check_fixed_sizes::<Granule8>();
    check_fixed_sizes::<Granule16>();
}

fn check_partial_accesses<L: Layout>() {
    let values = (0..=L::GRANULE as i8).chain([-128, -15, -14, -13, -11, -8, -1]);
    for first in values.clone() {
        for second in values.clone() {
            let shadow = [first, second, 0];
            for offset in 0..3 * L::GRANULE {
                for size in 1..=3 * L::GRANULE - offset {
                    let expected = (offset..offset + size)
                        .find(|&byte| {
                            let value = shadow[byte / L::GRANULE];
                            value < 0 || (value > 0 && byte % L::GRANULE >= value as usize)
                        })
                        .map(|byte| L::APPLICATION.start + byte);
                    let actual = shadow[offset / L::GRANULE..(offset + size).div_ceil(L::GRANULE)]
                        .find_invalid_shadow_byte::<L>(
                            L::APPLICATION.start + offset,
                            L::APPLICATION.start + offset + size - 1,
                        );
                    assert_eq!(
                        actual,
                        expected,
                        "granule={}, shadow={shadow:?}, offset={offset}, size={size}",
                        L::GRANULE
                    );
                }
            }
        }
    }
}

#[test]
fn slices_match_bytewise_checks_for_unaligned_and_partial_accesses() {
    check_partial_accesses::<Granule8>();
    check_partial_accesses::<Granule16>();
}

fn check_large_access<L: Layout>() {
    let mut shadow = [0; 128];
    shadow[64] = -15;
    assert_eq!(
        shadow[..].find_invalid_shadow_byte::<L>(
            L::APPLICATION.start,
            L::APPLICATION.start + shadow.len() * L::GRANULE - 1
        ),
        Some(L::APPLICATION.start + 64 * L::GRANULE),
    );
}

#[test]
fn finds_poison_in_the_middle_of_a_large_access() {
    check_large_access::<Granule8>();
    check_large_access::<Granule16>();
}

fn check_empty_and_unsupported<L: Layout>() {
    for (addr, size) in [
        (L::APPLICATION.start, 0),
        (usize::MAX, 0),
        (0, 16),
        (0x2000_0000, 0x20000),
        (L::APPLICATION.start - 1, 1),
        (L::APPLICATION.end, 16),
        (0x4000_0000, 4),
        (usize::MAX, 1),
    ] {
        unsafe { check_access::<L, 0>(addr, size, false) };
    }
}

#[test]
fn skips_empty_and_non_sram_accesses_without_reading_shadow() {
    check_empty_and_unsupported::<Granule8>();
    check_empty_and_unsupported::<Granule16>();
}

#[test]
#[should_panic(expected = "ASan: load of 2 byte(s)")]
fn check_access_reports_wrapping_ranges_without_reading_shadow() {
    unsafe { check_access_fixed::<Granule8, 2>(usize::MAX, false) };
}
