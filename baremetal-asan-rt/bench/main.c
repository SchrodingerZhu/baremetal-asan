#include <stddef.h>
#include <stdint.h>

extern long __llvm_libc_stdio_write(void *, const char *, size_t);
extern _Noreturn void __llvm_libc_exit(int);
typedef void (*check_fn)(uintptr_t, size_t);
extern unsigned scan_byte(const uint8_t *, unsigned);
extern unsigned scan_word(const uint8_t *, unsigned);
extern unsigned scan_word_unaligned(const uint8_t *, unsigned);
extern unsigned scan_mve(const uint8_t *, unsigned);
extern void range_byte(uintptr_t, size_t), range_word(uintptr_t, size_t), range_mve(uintptr_t, size_t);
extern void range_word_unaligned(uintptr_t, size_t);
extern void __asan_loadN(uintptr_t, size_t), baseline_loadN(uintptr_t, size_t);

#define FIXED(N) \
extern void __asan_load##N(uintptr_t), __asan_store##N(uintptr_t); \
extern void baseline_load##N(uintptr_t); \
static void fixed##N(uintptr_t a, size_t n) { (void)n; __asan_load##N(a); } \
static void old##N(uintptr_t a, size_t n) { (void)n; baseline_load##N(a); }
FIXED(1) FIXED(2) FIXED(4) FIXED(8) FIXED(16)

static volatile unsigned sink;
static void bytes(uintptr_t p, size_t n) { sink = scan_byte((const uint8_t *)p, n); }
static void words(uintptr_t p, size_t n) { sink = scan_word((const uint8_t *)p, n); }
static void unaligned_words(uintptr_t p, size_t n) { sink = scan_word_unaligned((const uint8_t *)p, n); }
static void vectors(uintptr_t p, size_t n) { sink = scan_mve((const uint8_t *)p, n); }
__attribute__((noinline)) static void empty(uintptr_t a, size_t n) {
    __asm__ volatile("" : : "r"(a), "r"(n) : "memory");
}

check_fn volatile isr_fn;
uintptr_t volatile isr_addr;
size_t volatile isr_size;

#define REG32(addr) (*(volatile uint32_t *)(addr))
#define CYCLES REG32(0xE0001004u)
#define DTCM ((uint8_t *)0x20008000u)
#define APP 0x20040000u
static _Alignas(16) uint8_t sram[8192];

static char line[192];
static unsigned pos;
static void text(const char *s) { while (*s) line[pos++] = *s++; }
static void number(uint32_t n) {
    char digits[10]; unsigned count = 0;
    do { digits[count++] = '0' + n % 10; n /= 10; } while (n);
    while (count) line[pos++] = digits[--count];
}
static void flush(void) {
    line[pos++] = '\n';
    __llvm_libc_stdio_write(0, line, pos);
    pos = 0;
}
static void row(const char *kind, const char *name, const char *memory,
                unsigned size, unsigned offset, unsigned cycles, unsigned iterations) {
    text(kind); text(","); text(name); text(","); text(memory); text(",");
    number(size); text(","); number(offset); text(","); number(cycles); text(",");
    number(iterations); flush();
}

_Noreturn void benchmark_failure(uintptr_t a, size_t n, int write, uintptr_t bad) {
    text("FAIL,"); number(a); text(","); number(n); text(","); number(write);
    text(","); number(bad); flush();
    __llvm_libc_exit(1);
}

// No semihost calls inside a timed region. Median of five warmed batches.
__attribute__((noinline)) static unsigned measure(check_fn fn, uintptr_t a, size_t n) {
    unsigned samples[5];
    for (unsigned i = 0; i < 32; ++i) fn(a, n);
    for (unsigned sample = 0; sample < 5; ++sample) {
        __asm__ volatile("dsb\n\tisb" ::: "memory");
        unsigned begin = CYCLES;
        for (unsigned i = 0; i < 512; ++i) fn(a, n);
        __asm__ volatile("dsb\n\tisb" ::: "memory");
        samples[sample] = CYCLES - begin;
    }
    for (unsigned i = 1; i < 5; ++i)
        for (unsigned j = i; j && samples[j] < samples[j - 1]; --j) {
            unsigned t = samples[j]; samples[j] = samples[j - 1]; samples[j - 1] = t;
        }
    return samples[2];
}

static void set_fp_active(unsigned active) {
    uint32_t control;
    __asm__ volatile("mrs %0, control" : "=r"(control));
    control = (control & ~4u) | (active ? 4u : 0);
    __asm__ volatile("msr control, %0\n\tisb" : : "r"(control) : "memory");
}

// Minimum of repeated single calls isolates the lazy-save effect without
// semihosting or the CONTROL write inside the measured interval.
static unsigned measure_exception(check_fn fn, uintptr_t addr, unsigned n, unsigned active) {
    isr_fn = fn; isr_addr = addr; isr_size = n;
    unsigned best = ~0u;
    for (unsigned i = 0; i < 65; ++i) {
        __asm__ volatile("vmov.i32 q0, #0" ::: "q0");
        set_fp_active(active);
        __asm__ volatile("dsb\n\tisb" ::: "memory");
        unsigned begin = CYCLES;
        __asm__ volatile("svc #0" ::: "memory");
        unsigned elapsed = CYCLES - begin;
        if (elapsed < best) best = elapsed;
    }
    return best;
}

static void validate_scans(uint8_t *base) {
    for (unsigned offset = 0; offset < 16; ++offset) {
        for (unsigned n = 1; n <= 65; ++n) {
            if (scan_byte(base + offset, n) || scan_word(base + offset, n) || scan_word_unaligned(base + offset, n) || scan_mve(base + offset, n))
                benchmark_failure(offset, n, 0, 0);
            for (unsigned poison = 0; poison < n; ++poison) {
                base[offset + poison] = 0xf1;
                if (!scan_byte(base + offset, n) || !scan_word(base + offset, n) || !scan_word_unaligned(base + offset, n) || !scan_mve(base + offset, n))
                    benchmark_failure(offset, n, 0, poison);
                base[offset + poison] = 0;
            }
            // Inactive vector lanes must not affect the result.
            base[offset + n] = 0xf1;
            if (scan_byte(base + offset, n) || scan_word(base + offset, n) || scan_word_unaligned(base + offset, n) || scan_mve(base + offset, n))
                benchmark_failure(offset, n, 0, n);
            base[offset + n] = 0;
        }
    }
}

int main(void) {
    if (*(volatile uint8_t *)0x4001E026u != 5 || REG32(0x4001E0ACu) != 0xF902u ||
        *(volatile uint16_t *)0x4001E04Cu != 0x0731u || REG32(0x4001E020u) != 0x32233432u ||
        *(volatile uint16_t *)0x4001E024u != 0x2220u || (REG32(0xE000ED14u) & ((1u << 16) | (1u << 3))))
        benchmark_failure(0, 0, 0, 1);
    // Enable and ECC-initialize DTCM with 64-bit stores before byte reads.
    REG32(0xE001E014u) |= 1u;
    __asm__ volatile("dsb\n\tisb" ::: "memory");
    for (unsigned i = 0; i < 128 * 1024 / 8; ++i)
        ((volatile uint64_t *)0x20000000u)[i] = 0;
    __asm__ volatile("dsb" ::: "memory");
    // Automatic FP/MVE context tracking with lazy preservation.
    REG32(0xE000EF34u) |= 0xC0000000u;
    text("CONFIG,cpu_hz,1000000000,icache,1,dcache,0,shadow,dtcm"); flush();
    text("CONFIG,fpccr,"); number(REG32(0xE000EF34u)); text(",cpdlpstate,");
    number(REG32(0xE000EDB8u)); flush();
    validate_scans(DTCM); validate_scans(sram);
    __asan_store1(APP); __asan_store2(APP + 7); __asan_store4(APP + 7);
    __asan_store8(APP + 7); __asan_store16(APP + 7);
    text("VALIDATION,passed"); flush();
    text("kind,implementation,memory,size,offset,cycles,iterations"); flush();
    row("overhead", "empty", "none", 0, 0, measure(empty, APP, 1), 512);

    const unsigned fixed_sizes[] = {1, 2, 4, 8, 16};
    const check_fn fixed[] = {fixed1, fixed2, fixed4, fixed8, fixed16};
    const check_fn old[] = {old1, old2, old4, old8, old16};
    for (unsigned i = 0; i < 5; ++i) for (unsigned offset = 0; offset <= 7; offset += 7) {
        row("fixed", "baseline", "dtcm", fixed_sizes[i], offset, measure(old[i], APP + offset, fixed_sizes[i]), 512);
        row("fixed", "specialized", "dtcm", fixed_sizes[i], offset, measure(fixed[i], APP + offset, fixed_sizes[i]), 512);
    }
    const unsigned sizes[] = {1, 2, 3, 4, 6, 8, 12, 16, 24, 32, 48, 64, 96, 128, 256, 512};
    const unsigned offsets[] = {0, 1, 3, 15};
    const check_fn scans[] = {bytes, words, unaligned_words, vectors};
    const char *names[] = {"byte", "word", "word_unaligned", "mve"};
    for (unsigned memory = 0; memory < 2; ++memory)
        for (unsigned i = 0; i < sizeof(sizes) / sizeof(*sizes); ++i)
            for (unsigned offset = 0; offset < 4; ++offset)
                for (unsigned impl = 0; impl < 4; ++impl)
                    row("scan", names[impl], memory ? "sram" : "dtcm", sizes[i], offsets[offset],
                        measure(scans[impl], (uintptr_t)(memory ? sram : DTCM) + offsets[offset], sizes[i]), 512);

    const check_fn ranges[] = {__asan_loadN, range_byte, range_word, range_word_unaligned, range_mve};
    const char *range_names[] = {"current", "byte", "word", "word_unaligned", "mve"};
    for (unsigned i = 0; i < sizeof(sizes) / sizeof(*sizes); ++i)
        for (unsigned offset = 0; offset < 4; ++offset)
            for (unsigned impl = 0; impl < 5; ++impl)
                row("range", range_names[impl], "dtcm", sizes[i] * 8, offsets[offset],
                    measure(ranges[impl], APP + offsets[offset], sizes[i] * 8), 512);

    for (unsigned active = 0; active < 2; ++active) {
        row("svc", "empty", active ? "fp_active" : "fp_inactive", 0, 0, measure_exception(empty, (uintptr_t)DTCM, 0, active), 1);
        for (unsigned n = 4; n <= 256; n *= 4)
            for (unsigned impl = 0; impl < 4; ++impl)
                row("svc", names[impl], active ? "fp_active" : "fp_inactive", n, 0,
                    measure_exception(scans[impl], (uintptr_t)DTCM, n, active), 1);
        const unsigned range_sizes[] = {32, 64, 128, 256, 512, 1024, 1536, 2048, 3072, 4096};
        for (unsigned i = 0; i < sizeof(range_sizes) / sizeof(*range_sizes); ++i)
            for (unsigned impl = 0; impl < 5; ++impl)
                row("svc_range", range_names[impl], active ? "fp_active" : "fp_inactive", range_sizes[i], 0,
                    measure_exception(ranges[impl], APP, range_sizes[i], active), 1);
    }
    text("DONE"); flush();
    return 0;
}
