#include <arm_mve.h>
#include <stddef.h>
#include <stdint.h>

// These scan only zero/nonzero shadow bytes. The range wrappers below handle
// partial application granules by falling back to the exact Rust checker.
unsigned scan_byte(const uint8_t *p, unsigned n) {
    for (unsigned i = 0; i < n; ++i)
        if (((const volatile uint8_t *)p)[i]) return 1;
    return 0;
}

unsigned scan_word(const uint8_t *p, unsigned n) {
    while (n && ((uintptr_t)p & 3)) {
        if (*(const volatile uint8_t *)p++) return 1;
        --n;
    }
    while (n >= 4) {
        if (*(const volatile uint32_t *)p) return 1;
        p += 4;
        n -= 4;
    }
    while (n--) if (*(const volatile uint8_t *)p++) return 1;
    return 0;
}

// Cortex-M85 normal RAM permits unaligned word loads when UNALIGN_TRP is clear.
// The packed type avoids imposing a C alignment requirement on the pointer.
struct __attribute__((packed)) unaligned_word { uint32_t value; };
unsigned scan_word_unaligned(const uint8_t *p, unsigned n) {
    while (n >= 4) {
        if (((const volatile struct unaligned_word *)p)->value) return 1;
        p += 4;
        n -= 4;
    }
    while (n--) if (*(const volatile uint8_t *)p++) return 1;
    return 0;
}

unsigned scan_mve(const uint8_t *p, unsigned n) {
    while (n) {
        mve_pred16_t active = vctp8q(n);
        uint8x16_t value = vldrbq_z_u8(p, active);
        if (vcmpneq_n_u8(value, 0)) return 1;
        if (n <= 16) break;
        p += 16;
        n -= 16;
    }
    return 0;
}

extern void __asan_loadN(uintptr_t, size_t);

#define RANGE_CHECK(name, scan) \
void name(uintptr_t addr, size_t size) { \
    if (!size) return; \
    uintptr_t last; \
    if (__builtin_add_overflow(addr, size - 1, &last)) { \
        __asan_loadN(addr, size); return; \
    } \
    uintptr_t first = addr < 0x20040000u ? 0x20040000u : addr; \
    if (last > 0x201fffffu) last = 0x201fffffu; \
    if (first > last) return; \
    const uint8_t *shadow = (const uint8_t *)(0x20000000u + ((first - 0x20000000u) >> 3)); \
    unsigned count = (last >> 3) - (first >> 3) + 1; \
    if (scan(shadow, count)) __asan_loadN(addr, size); \
}

RANGE_CHECK(range_byte, scan_byte)
RANGE_CHECK(range_word, scan_word)
RANGE_CHECK(range_word_unaligned, scan_word_unaligned)
RANGE_CHECK(range_mve, scan_mve)
