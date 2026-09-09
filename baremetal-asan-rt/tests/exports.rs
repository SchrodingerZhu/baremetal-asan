use baremetal_asan_rt as runtime;
use core::ops::Range;
use runtime::layout::Layout;

struct TestLayout<const SCALE: u32>;
impl<const SCALE: u32> Layout for TestLayout<SCALE> {
    const APPLICATION: Range<usize> = 0x1000..0x2000;
    const SHADOW_SCALE: u32 = SCALE;
    const SHADOW_BASE: usize = 0x3000;
}

fn stack_top() -> usize {
    0
}

runtime::export_asan!(TestLayout<3>, stack_top = stack_top);

#[test]
fn exports_link_with_a_custom_layout_and_stack_provider() {
    __asan_version_mismatch_check_v8();
    assert_eq!(__asan_stack_malloc_0(16), 0);
    assert_eq!(__asan_stack_malloc_always_10(65536), 0);
    __asan_stack_free_10(0, 0);
    let mut flag = 42;
    __asan_register_elf_globals(&mut flag, core::ptr::null_mut(), core::ptr::null_mut());
    assert_eq!(flag, 42);
    // Empty/unsupported accesses and rejected setters must not dereference the
    // synthetic layout's addresses. The custom stack bound disables cleanup.
    unsafe {
        __asan_load1(0);
        __asan_store16(0);
        __asan_loadN(usize::MAX, 0);
        __asan_set_shadow_f1(0, 1);
        __asan_set_shadow_00(0x3000, 0);
        __asan_handle_no_return();
        assert_eq!(
            core::ptr::read(core::ptr::addr_of!(
                __asan_option_detect_stack_use_after_return
            )),
            0
        );
    }
}

#[test]
fn memory_exports_forward_to_the_generic_runtime() {
    let src = *b"12345678";
    let mut dst = [0; 8];
    unsafe {
        assert_eq!(
            __asan_memcpy(dst.as_mut_ptr().cast(), src.as_ptr().cast(), 8),
            dst.as_mut_ptr().cast()
        );
        assert_eq!(&dst, &src);
        __asan_memmove(dst.as_mut_ptr().add(1).cast(), dst.as_ptr().cast(), 7);
        assert_eq!(&dst, b"11234567");
        __asan_memset(dst.as_mut_ptr().cast(), 0x141, 8);
        assert_eq!(&dst, b"AAAAAAAA");
    }
}
