use super::*;
use crate::test_layout::Granule8;

#[test]
fn memcpy_copies_only_the_requested_bytes_and_returns_destination() {
    let src = *b"01234567";
    let mut dst = [0xff; 10];
    let target = unsafe { dst.as_mut_ptr().add(1) }.cast();
    assert_eq!(
        unsafe { memcpy::<Granule8>(target, src.as_ptr().cast(), src.len()) },
        target
    );
    assert_eq!(&dst[1..9], &src);
    assert_eq!(dst[0], 0xff);
    assert_eq!(dst[9], 0xff);
}

#[test]
fn memmove_supports_overlap_in_both_directions() {
    let mut bytes = *b"0123456789";
    let p = bytes.as_mut_ptr();
    assert_eq!(
        unsafe { memmove::<Granule8>(p.add(2).cast(), p.cast(), 8) },
        unsafe { p.add(2).cast() }
    );
    assert_eq!(&bytes, b"0101234567");

    bytes = *b"0123456789";
    let p = bytes.as_mut_ptr();
    assert_eq!(
        unsafe { memmove::<Granule8>(p.cast(), p.add(2).cast(), 8) },
        p.cast()
    );
    assert_eq!(&bytes, b"2345678989");
}

#[test]
fn memset_truncates_to_a_byte_and_preserves_surrounding_bytes() {
    for value in [-1, 0x123] {
        let mut bytes = [0x55; 9];
        let dst = unsafe { bytes.as_mut_ptr().add(1) }.cast();
        assert_eq!(unsafe { memset::<Granule8>(dst, value, 7) }, dst);
        assert_eq!(&bytes[1..8], &[value as u8; 7]);
        assert_eq!(bytes[0], 0x55);
        assert_eq!(bytes[8], 0x55);
    }
}

#[test]
fn identical_copy_ranges_are_allowed() {
    let mut bytes = *b"same buffer";
    let p = bytes.as_mut_ptr().cast();
    assert_eq!(unsafe { memcpy::<Granule8>(p, p, bytes.len()) }, p);
    assert_eq!(unsafe { memmove::<Granule8>(p, p, bytes.len()) }, p);
    assert_eq!(&bytes, b"same buffer");
}

#[test]
fn zero_length_operations_accept_null_pointers() {
    let p = ptr::null_mut();
    assert_eq!(unsafe { memcpy::<Granule8>(p, p, 0) }, p);
    assert_eq!(unsafe { memmove::<Granule8>(p, p, 0) }, p);
    assert_eq!(unsafe { memset::<Granule8>(p, -1, 0) }, p);
}
