use super::*;

const APP_START: usize = RAM_START + RAM_OFFSET;
const RAM_END: usize = RAM_START + RAM_SIZE;

fn check_fixed_size<const SIZE: usize>() {
    for slot in 0..3 {
        for value in i8::MIN..=i8::MAX {
            let mut shadow = [0; 3];
            shadow[slot] = value;
            for offset in 0..8 {
                let expected = (offset..offset + SIZE)
                    .find(|&byte| {
                        let value = shadow[byte / 8];
                        value < 0 || (value > 0 && byte % 8 >= value as usize)
                    })
                    .map(|byte| APP_START + byte);
                assert_eq!(
                    first_poisoned(
                        APP_START + offset,
                        APP_START + offset + SIZE - 1,
                        &shadow[..(offset + SIZE).div_ceil(8)],
                    ),
                    expected,
                    "size={SIZE}, shadow={shadow:?}, offset={offset}",
                );
                let first = APP_START + offset;
                let last = first + SIZE - 1;
                let actual = match (offset + SIZE).div_ceil(8) {
                    1 => first_poisoned(first, last, shadow.first_chunk::<1>().unwrap()),
                    2 => first_poisoned(first, last, shadow.first_chunk::<2>().unwrap()),
                    3 => first_poisoned(first, last, &shadow),
                    _ => unreachable!(),
                };
                assert_eq!(
                    actual, expected,
                    "fixed size={SIZE}, shadow={shadow:?}, offset={offset}",
                );
            }
        }
    }
    for boundary in [APP_START, RAM_END, usize::MAX - 16] {
        for addr in boundary - 16..=boundary + 16 {
            let Some(last) = addr.checked_add(SIZE - 1) else {
                assert!(slice_to_shadow_slice(addr, SIZE).is_none());
                continue;
            };
            let first = addr.max(APP_START);
            let last = last.min(RAM_END - 1);
            if first > last {
                assert!(slice_to_shadow_slice(addr, SIZE).is_none());
                continue;
            }
            let shadow_len = last / 8 - first / 8 + 1;
            for value in [0, 1, 7, -15] {
                let expected = (first..=last)
                    .find(|&byte| value < 0 || (value > 0 && byte % 8 >= value as usize));
                assert_eq!(
                    first_poisoned(first, last, &[value; 3][..shadow_len]),
                    expected,
                    "size={SIZE}, addr={addr:#x}, shadow={value}",
                );
            }
        }
    }
}

#[test]
fn fixed_sizes_match_bytewise_checks_and_boundary_handling() {
    check_fixed_size::<1>();
    check_fixed_size::<2>();
    check_fixed_size::<4>();
    check_fixed_size::<8>();
    check_fixed_size::<16>();
}

#[test]
fn shadow_mapping_stays_within_reserved_ram() {
    assert_eq!(addr_to_shadow(APP_START), Some(0x2000_8000));
    assert_eq!(addr_to_shadow(APP_START + 7), Some(0x2000_8000));
    assert_eq!(addr_to_shadow(APP_START + 8), Some(0x2000_8001));
    assert_eq!(addr_to_shadow(RAM_END - 1), Some(0x2003_ffff));
    assert_eq!(addr_to_shadow(APP_START - 1), None);
    assert_eq!(addr_to_shadow(RAM_END), None);
}

#[test]
fn matches_bytewise_checks_for_unaligned_and_partial_accesses() {
    let values = [
        0, 1, 2, 3, 4, 5, 6, 7, -128, -15, -14, -13, -11, -8, -6, -3, -1,
    ];
    for first in values {
        for second in values {
            let shadow = [first, second, 0];
            for offset in 0..24 {
                for size in 1..=24 - offset {
                    // Independent reference: check every application byte.
                    let expected = (offset..offset + size)
                        .find(|&byte| {
                            let value = shadow[byte / 8];
                            value < 0 || (value > 0 && byte % 8 >= value as usize)
                        })
                        .map(|byte| APP_START + byte);
                    let actual = first_poisoned(
                        APP_START + offset,
                        APP_START + offset + size - 1,
                        &shadow[offset / 8..(offset + size).div_ceil(8)],
                    );
                    assert_eq!(
                        actual, expected,
                        "shadow={shadow:?}, offset={offset}, size={size}"
                    );
                }
            }
        }
    }
}

#[test]
fn finds_poison_in_the_middle_of_a_large_access() {
    let mut shadow = [0; 128];
    shadow[64] = -15;
    assert_eq!(
        first_poisoned(APP_START, APP_START + 1023, &shadow[..]),
        Some(APP_START + 512),
    );
}

#[test]
fn skips_empty_and_non_sram_accesses_without_reading_shadow() {
    for (addr, size) in [
        (APP_START, 0),
        (usize::MAX, 0),
        (0, 16),
        (RAM_START, RAM_OFFSET),
        (APP_START - 1, 1),
        (RAM_END, 16),
        (0x4000_0000, 4),
        (usize::MAX, 1),
    ] {
        assert!(slice_to_shadow_slice(addr, size).is_none());
        check_access(addr, size, false, slice_to_shadow_slice(addr, size));
    }
}

#[test]
fn checks_only_the_sram_overlap_at_both_boundaries() {
    assert_eq!(first_poisoned(APP_START, APP_START + 3, &[0]), None);
    assert_eq!(
        first_poisoned(APP_START, APP_START + 3, &[-15]),
        Some(APP_START)
    );
    assert_eq!(
        first_poisoned(APP_START, APP_START + 3, &[3]),
        Some(APP_START + 3)
    );
    assert_eq!(first_poisoned(APP_START, APP_START + 3, &[4]), None);

    assert_eq!(first_poisoned(RAM_END - 4, RAM_END - 1, &[0]), None);
    assert_eq!(
        first_poisoned(RAM_END - 4, RAM_END - 1, &[-15]),
        Some(RAM_END - 4)
    );
    assert_eq!(
        first_poisoned(RAM_END - 4, RAM_END - 1, &[7]),
        Some(RAM_END - 1)
    );
}

#[test]
fn rejects_wrapping_ranges_without_reading_shadow() {
    assert!(slice_to_shadow_slice(usize::MAX, 2).is_none());
    assert!(slice_to_shadow_slice(APP_START, usize::MAX).is_none());
}

#[test]
#[should_panic(expected = "ASan: load of 2 byte(s)")]
fn check_access_reports_wrapping_ranges_without_a_shadow_slice() {
    check_access_fixed::<2>(usize::MAX, false);
}
