use super::*;

const APP_START: usize = RAM_START + RAM_OFFSET;
const RAM_END: usize = RAM_START + RAM_SIZE;

fn check_fixed_size<const SIZE: usize>() {
    let shadow_start = addr_to_shadow(APP_START).unwrap();
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
                    first_poisoned_fixed::<SIZE>(APP_START + offset, |address| {
                        shadow[address - shadow_start]
                    }),
                    expected,
                    "size={SIZE}, shadow={shadow:?}, offset={offset}",
                );
            }
        }
    }
    for boundary in [APP_START, RAM_END, usize::MAX - 16] {
        for addr in boundary - 16..=boundary + 16 {
            for value in [0, 1, 7, -15] {
                assert_eq!(
                    first_poisoned_fixed::<SIZE>(addr, |_| value),
                    first_poisoned(addr, SIZE, |_| value),
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
    let shadow_start = addr_to_shadow(APP_START).unwrap();
    let values = [
        0, 1, 2, 3, 4, 5, 6, 7, -128, -15, -14, -13, -11, -8, -6, -3, -1,
    ];
    for first in values {
        for second in values {
            let shadow = [first, second, 0];
            for offset in 0..24 {
                for size in 0..=24 - offset {
                    // Independent reference: check every application byte.
                    let expected = (offset..offset + size)
                        .find(|&byte| {
                            let value = shadow[byte / 8];
                            value < 0 || (value > 0 && byte % 8 >= value as usize)
                        })
                        .map(|byte| APP_START + byte);
                    let actual = first_poisoned(APP_START + offset, size, |address| {
                        shadow[address - shadow_start]
                    });
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
    let shadow_start = addr_to_shadow(APP_START).unwrap();
    assert_eq!(
        first_poisoned(APP_START, 1024, |address| {
            if address == shadow_start + 64 { -15 } else { 0 }
        }),
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
        assert_eq!(
            first_poisoned(addr, size, |_| panic!("unexpected shadow read")),
            None
        );
    }
}

#[test]
fn checks_only_the_sram_overlap_at_both_boundaries() {
    let mut reads = 0;
    assert_eq!(
        first_poisoned(APP_START - 4, 8, |address| {
            assert_eq!(address, addr_to_shadow(APP_START).unwrap());
            reads += 1;
            0
        }),
        None,
    );
    assert_eq!(reads, 1);
    assert_eq!(first_poisoned(APP_START - 4, 8, |_| -15), Some(APP_START));

    reads = 0;
    assert_eq!(
        first_poisoned(RAM_END - 4, 8, |address| {
            assert_eq!(address, RAM_START + RAM_OFFSET - 1);
            reads += 1;
            0
        }),
        None,
    );
    assert_eq!(reads, 1);
    assert_eq!(first_poisoned(RAM_END - 4, 8, |_| -15), Some(RAM_END - 4));
}

#[test]
fn rejects_wrapping_ranges_without_reading_shadow() {
    assert_eq!(
        first_poisoned(usize::MAX, 2, |_| panic!("unexpected shadow read")),
        Some(usize::MAX),
    );
    assert_eq!(
        first_poisoned(APP_START, usize::MAX, |_| panic!("unexpected shadow read")),
        Some(APP_START),
    );
}
