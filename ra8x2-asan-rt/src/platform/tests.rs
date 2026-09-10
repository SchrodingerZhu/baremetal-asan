use super::*;

#[test]
fn granule_eight_splits_at_the_end_of_dtcm() {
    let mut parts = Ra8m2Granule8::to_ranges(0x2210_0000 - 4, 12);
    assert_eq!(
        parts.next(),
        Some(Shadow {
            memory: 0x220f_fffc..0x2210_0000,
            bytes: 0x2001_ffff..0x2002_0000,
        })
    );
    assert_eq!(
        parts.next(),
        Some(Shadow {
            memory: 0x2210_0000..0x2210_0008,
            bytes: 0x2218_c000..0x2218_c001,
        })
    );
    assert_eq!(parts.next(), None);
    assert_eq!(Ra8m2Granule8::SHADOW_OFFSET, 0x1bc0_0000);
}

#[test]
fn full_application_shadow_fits_each_layout() {
    let mut eight = Ra8m2Granule8::to_ranges(0, usize::MAX);
    assert_eq!(
        eight.next(),
        Some(Shadow {
            memory: 0x2200_0000..0x2210_0000,
            bytes: 0x2000_0000..0x2002_0000,
        })
    );
    assert_eq!(
        eight.next(),
        Some(Shadow {
            memory: 0x2210_0000..0x2218_c000,
            bytes: 0x2218_c000..0x2219_d800,
        })
    );
    assert_eq!(eight.next(), None);

    let mut sixteen = Ra8m2Granule16::to_ranges(0, usize::MAX);
    assert_eq!(
        sixteen.next(),
        Some(Shadow {
            memory: 0x2200_0000..0x221a_0000,
            bytes: 0x2000_0000..0x2001_a000,
        })
    );
    assert_eq!(sixteen.next(), None);
    assert_eq!(Ra8m2Granule16::SHADOW_OFFSET, 0x1de0_0000);
}

#[test]
fn sram_only_layout_keeps_all_shadow_out_of_dtcm() {
    let mut pieces = Ra8m2Granule8Sram::to_ranges(0, usize::MAX);
    assert_eq!(
        pieces.next(),
        Some(Shadow {
            memory: 0x2200_0000..0x2217_0000,
            bytes: 0x2217_0000..0x2219_e000,
        })
    );
    assert_eq!(pieces.next(), None);
    assert_eq!(Ra8m2Granule8Sram::SHADOW_OFFSET, 0x1dd7_0000);
    assert_eq!(Ra8m2Granule8Sram::APPLICATION.len(), 1472 * 1024);
    assert_eq!(Ra8m2Granule8Sram::SHADOW_SIZE, 184 * 1024);
    assert!(
        Ra8m2Granule8Sram::to_ranges(0x2000_0000, 0x20000)
            .next()
            .is_none()
    );
}

fn check_boundaries<P: Platform>() {
    for boundary in [
        P::APPLICATION.start,
        0x2210_0000,
        P::APPLICATION.end,
        usize::MAX - 32,
    ] {
        for addr in boundary - 32..=boundary + 32 {
            for size in 0..=32 {
                let end = addr.checked_add(size);
                let first = addr.max(P::APPLICATION.start);
                let end = end.unwrap_or(0).min(P::APPLICATION.end);
                let mut next = first;
                for part in P::to_ranges(addr, size) {
                    assert!(size != 0 && first < end);
                    assert_eq!(part.memory.start, next);
                    assert!(part.memory.end <= end);
                    assert!(!part.bytes.is_empty());
                    assert_eq!(
                        part.bytes.len(),
                        (part.memory.end - P::APPLICATION.start).div_ceil(P::GRANULE)
                            - (part.memory.start - P::APPLICATION.start) / P::GRANULE
                    );
                    assert!(
                        P::to_ranges(0, usize::MAX)
                            .any(|region| region.bytes.start <= part.bytes.start
                                && part.bytes.end <= region.bytes.end)
                    );
                    next = part.memory.end;
                }
                if size != 0 && first < end {
                    assert_eq!(next, end);
                }
            }
        }
    }
}

#[test]
fn pieces_cover_only_the_supported_overlap_at_boundaries() {
    check_boundaries::<Ra8m2Granule8>();
    check_boundaries::<Ra8m2Granule16>();
    check_boundaries::<Ra8m2Granule8Sram>();
}

#[test]
fn wrapping_ranges_have_no_shadow() {
    assert!(Ra8m2Granule8Sram::to_ranges(usize::MAX, 2).next().is_none());
    assert!(Ra8m2Granule8::to_ranges(usize::MAX, 2).next().is_none());
    assert!(
        Ra8m2Granule16::to_ranges(0x2200_0000, usize::MAX)
            .next()
            .is_none()
    );
}

#[test]
fn active_platform_matches_feature_selection() {
    if cfg!(feature = "granule-16") {
        assert_eq!(ActivePlatform::GRANULE, 16);
        assert_eq!(ActivePlatform::SHADOW_BASE, 0x2000_0000);
    } else if cfg!(feature = "no-dtcm") {
        assert_eq!(ActivePlatform::GRANULE, 8);
        assert_eq!(ActivePlatform::SHADOW_BASE, 0x2217_0000);
    } else {
        assert_eq!(ActivePlatform::GRANULE, 8);
        assert_eq!(ActivePlatform::SHADOW_BASE, 0x2000_0000);
    }
}
