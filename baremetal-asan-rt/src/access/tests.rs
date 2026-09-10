use super::*;
use crate::test_platform::{Granule8, Granule16};

fn check_fixed_size<P: Platform, const SIZE: usize>() {
    let start = P::APPLICATION.start;
    for slot in 0..3 {
        for value in i8::MIN..=i8::MAX {
            let mut shadow = [0; 3];
            shadow[slot] = value;
            for offset in 0..P::GRANULE {
                // Independent reference: check every application byte.
                let expected = (offset..offset + SIZE)
                    .find(|&byte| {
                        let value = shadow[byte / P::GRANULE];
                        value < 0 || (value > 0 && byte % P::GRANULE >= value as usize)
                    })
                    .map(|byte| start + byte);
                let first = start + offset;
                let last = first + SIZE - 1;
                let count = (offset + SIZE).div_ceil(P::GRANULE);
                assert_eq!(
                    shadow[..count].find_invalid_shadow_byte::<P>(first, last),
                    expected,
                    "size={SIZE}, granule={}, shadow={shadow:?}, offset={offset}",
                    P::GRANULE,
                );
                let actual = match count {
                    1 => shadow
                        .first_chunk::<1>()
                        .unwrap()
                        .find_invalid_shadow_byte::<P>(first, last),
                    2 => shadow
                        .first_chunk::<2>()
                        .unwrap()
                        .find_invalid_shadow_byte::<P>(first, last),
                    3 => shadow.find_invalid_shadow_byte::<P>(first, last),
                    _ => unreachable!(),
                };
                assert_eq!(actual, expected, "fixed size={SIZE}, offset={offset}");
            }
        }
    }
}

fn check_fixed_sizes<P: Platform>() {
    check_fixed_size::<P, 1>();
    check_fixed_size::<P, 2>();
    check_fixed_size::<P, 4>();
    check_fixed_size::<P, 8>();
    check_fixed_size::<P, 16>();
}

#[test]
fn fixed_sizes_match_bytewise_checks() {
    check_fixed_sizes::<Granule8>();
    check_fixed_sizes::<Granule16>();
}

fn check_partial_accesses<P: Platform>() {
    let values = (0..=P::GRANULE as i8).chain([-128, -15, -14, -13, -11, -8, -1]);
    for first in values.clone() {
        for second in values.clone() {
            let shadow = [first, second, 0];
            for offset in 0..3 * P::GRANULE {
                for size in 1..=3 * P::GRANULE - offset {
                    let expected = (offset..offset + size)
                        .find(|&byte| {
                            let value = shadow[byte / P::GRANULE];
                            value < 0 || (value > 0 && byte % P::GRANULE >= value as usize)
                        })
                        .map(|byte| P::APPLICATION.start + byte);
                    let actual = shadow[offset / P::GRANULE..(offset + size).div_ceil(P::GRANULE)]
                        .find_invalid_shadow_byte::<P>(
                            P::APPLICATION.start + offset,
                            P::APPLICATION.start + offset + size - 1,
                        );
                    assert_eq!(
                        actual,
                        expected,
                        "granule={}, shadow={shadow:?}, offset={offset}, size={size}",
                        P::GRANULE
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

fn check_large_access<P: Platform>() {
    let mut shadow = [0; 128];
    shadow[64] = -15;
    assert_eq!(
        shadow[..].find_invalid_shadow_byte::<P>(
            P::APPLICATION.start,
            P::APPLICATION.start + shadow.len() * P::GRANULE - 1
        ),
        Some(P::APPLICATION.start + 64 * P::GRANULE),
    );
}

#[test]
fn finds_poison_in_the_middle_of_a_large_access() {
    check_large_access::<Granule8>();
    check_large_access::<Granule16>();
}

fn check_empty_and_unsupported<P: Platform>() {
    for (addr, size) in [
        (P::APPLICATION.start, 0),
        (usize::MAX, 0),
        (0, 16),
        (0x2000_0000, 0x20000),
        (P::APPLICATION.start - 1, 1),
        (P::APPLICATION.end, 16),
        (0x4000_0000, 4),
        (usize::MAX, 1),
    ] {
        unsafe { check_access::<P, 0>(addr, size, false) };
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
