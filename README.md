# Bare-metal AddressSanitizer

This LLVM fork provides a `no_std` ASan runtime and compiler/linker support for
firmware with split RAM shadow and precomputed ROM shadow. It includes a portable
Rust library, an example Cortex-M85/RA8M2 static library, and the LLVM libc
allocator used for heap and fake-stack quarantine. The RA8M2 crate demonstrates
three memory layouts. Users can define their own `Platform` and linker
reservations for targets such as RP2040 and RP2350.

## What the project provides

✅ means implemented; ❌ means unavailable or supplied by the embedding
firmware. Brace notation groups ABI names; `0..10` includes both endpoints.

| Status | Feature | ABI / interface and scope |
| --- | --- | --- |
| ✅ | Outlined reads and writes | `__asan_{load,store}{1,2,4,8,16,N}`; fixed and variable sizes, including accesses crossing physical shadow regions. |
| ✅ | Checked memory operations | `__asan_memcpy`, `__asan_memmove`, `__asan_memset`; memcpy also diagnoses overlapping ranges. |
| ✅ | Stack redzones and cleanup | `__asan_set_shadow_{00,01,02,03,04,05,06,07,f1,f2,f3,f5,f8}`. |
| ✅ | Stack use after scope | `__asan_poison_stack_memory`, `__asan_unpoison_stack_memory`. |
| ✅ | Dynamic alloca / VLA redzones | `__asan_alloca_poison`, `__asan_allocas_unpoison`; these allocations remain on the real stack. |
| ✅ | Stack use after return | `__asan_stack_malloc_{0..10}`, `__asan_stack_malloc_always_{0..10}`, `__asan_stack_free_{0..10}`, and `__asan_option_detect_stack_use_after_return`. |
| ✅ | Heap redzones and use after free | Explicit `__asan_malloc` / `__asan_free`, sharing a quarantined arena with fake stacks. |
| ✅ | Raw allocator integration | `__baremetal_asan_heap_{init,allocate,deallocate}`; raw storage, without ASan redzones. Rust `Heap<P>` supplies critical-section protection. |
| ✅ | SRAM global redzones | `__asan_init_globals(start, end)` initializes shadow from linker-retained descriptors. |
| ✅ | ROM global redzones | LLVM emits static shadow, LLD places it, and `Platform::rom_shadow()` exposes it to checks. |
| ✅ | No-return stack cleanup | `__asan_handle_no_return` clears real-stack shadow up to `Platform::stack_top()`. |
| ✅ | ABI version marker | `__asan_version_mismatch_check_v8`; a no-op link-time compatibility symbol. |
| ✅ | Diagnostics | Failing access, application address, colored shadow bytes, and poison legend; RA8x2 uses a semihosting panic handler. |
| ❌ | Automatic board/shadow initialization | Firmware supplies `__asan_init` and calls it before instrumented code. |
| ❌ | Automatic global registration / unloading | `__asan_register_*` / `__asan_unregister_*` compatibility symbols are stubs; use explicit SRAM startup initialization. |
| ❌ | Global initialization-order checking | `__asan_before_dynamic_init` / `__asan_after_dynamic_init` are stubs. |
| ❌ | Inline-check report ABI and recovery | `__asan_report_*` and `*_noabort` are not exported. |
| ❌ | Standard allocation interception | No replacement `malloc`, `calloc`, `realloc`, `free`, or C++ `new` / `delete`; applications provide adapters. |
| ❌ | Invalid-free / double-free diagnostics | `__asan_free` requires a live allocation from this runtime, or null. |
| ❌ | Other compiler extensions | Pointer comparison/subtraction, intra-object padding, and `__asan_exp_*` experiment ABIs. |
| ❌ | Hosted runtime facilities | Leak detection, on-device symbolized backtraces, and fake-frame reclamation after nonlocal exits. |

Quarantine delays reuse, so temporal errors are detectable while the retired
storage remains poisoned. Fake-stack exhaustion lets LLVM fall back to the real
stack, reducing use-after-return coverage.

The [portable crate](baremetal-asan-rt/README.md) owns the algorithms and ABI export
macros. The [RA8x2 crate](ra8x2-asan-rt/README.md) chooses a platform and supplies the
panic handler and Cortex-M critical-section backend. See [spec.md](baremetal-asan-rt/spec.md)
for the wider compiler ABI inventory, including unimplemented functions.

## Why the shadow mapping is configurable

One shadow byte describes a granule of application memory: `00` means the whole
granule is accessible, a positive value gives the accessible prefix length, and
a negative byte marks poison. Eight-byte granules use one shadow byte per eight
application bytes.

A hosted linear mapping is usually written as:

```text
logical_shadow = (application_address >> scale) + offset
```

Firmware has two additional placement problems. Writable shadow may span memory
banks with an address gap between them. Constant globals may live in ROM, where
reserving RAM shadow and rebuilding it at startup is unnecessary.

### Split writable shadow: application → logical → physical

`Platform` separates three concepts:

| Address | Meaning |
| --- | --- |
| Application | The byte the instrumented program accesses. |
| Logical shadow | LLVM's linear shadow address; an identifier that need not be dereferenceable. |
| Physical shadow | The actual DTCM, SRAM, or ROM byte read or written by the runtime. |

Outlined access checkers receive application addresses. They use
`to_shadow_slices()` to visit the physical pieces of shadow in application order.
Each borrowed slice stays inside one physical region; no shadow bytes are copied.
Shadow setters receive **logical shadow addresses and shadow-byte counts**. The
runtime validates that logical RAM range, recovers the application granules, and
fills their writable physical slices.

The three example RA8M2 layouts below demonstrate different shadow placements.
All ranges have exclusive ends; these examples place application data in main
SRAM and use DTCM as shadow storage where selected. The portable runtime takes
these choices from `Platform`: it checks the application ranges described by the
mapping and skips unmapped addresses.

**Default: eight-byte granules, DTCM + SRAM shadow**

```text
┌────────────────────────┐   ┌────────────────────────┐   ┌────────────────────────┐
│    Application RAM     │   │     Logical shadow     │   │    Physical shadow     │
├────────────────────────┤   ├────────────────────────┤   ├────────────────────────┤
│ 0x22000000..0x22100000 │──▶│ 0x20000000..0x20020000 │──▶│ 0x20000000..0x20020000 │
│ 0x22100000..0x2218c000 │──▶│ 0x20020000..0x20031800 │──▶│ 0x2218c000..0x2219d800 │
└────────────────────────┘   └────────────────────────┘   └────────────────────────┘
```

**granule-16: sixteen-byte granules, all shadow in DTCM**

```text
┌────────────────────────┐   ┌────────────────────────┐   ┌────────────────────────┐
│    Application RAM     │   │     Logical shadow     │   │    Physical shadow     │
├────────────────────────┤   ├────────────────────────┤   ├────────────────────────┤
│ 0x22000000..0x221a0000 │──▶│ 0x20000000..0x2001a000 │──▶│ 0x20000000..0x2001a000 │
└────────────────────────┘   └────────────────────────┘   └────────────────────────┘
```

**no-dtcm: eight-byte granules, all shadow in SRAM**

```text
┌────────────────────────┐   ┌────────────────────────┐   ┌────────────────────────┐
│    Application RAM     │   │     Logical shadow     │   │    Physical shadow     │
├────────────────────────┤   ├────────────────────────┤   ├────────────────────────┤
│ 0x22000000..0x22170000 │──▶│ 0x22170000..0x2219e000 │──▶│ 0x22170000..0x2219e000 │
└────────────────────────┘   └────────────────────────┘   └────────────────────────┘
```

| Cargo selection | Application RAM | Shadow used | SRAM reserved for shadow | Scale | LLVM offset |
| --- | ---: | --- | ---: | ---: | --- |
| Default: `Ra8m2Granule8` | 1584 KiB | 128 KiB DTCM + 70 KiB SRAM | 80 KiB | 3 | `0x1bc00000` |
| `granule-16`: `Ra8m2Granule16` | 1664 KiB | 104 KiB DTCM | 0 | 4 | `0x1de00000` |
| `no-dtcm`: `Ra8m2Granule8Sram` | 1472 KiB | 184 KiB SRAM | 192 KiB | 3 | `0x1dd70000` |

The extra 10 KiB / 8 KiB in the SRAM reservations is unused padding. `granule-16`
and `no-dtcm` are mutually exclusive. The [platform implementations](ra8x2-asan-rt/src/platform.rs)
are the source of these bounds.

**Compiler contract:** this integration assumes fully outlined access checks and
shadow setters. Use `-fsanitize-address-outline-instrumentation` and
`-mllvm -asan-max-inline-poisoning-size=0`. Direct compiler shadow accesses bypass
the physical mapping. Compiler-generated `__asan_set_shadow_*` calls must describe
dynamic/stack storage in application RAM, including fake-stack frames. SRAM
globals use descriptor-driven startup poisoning; ROM globals use static shadow.
A logical RAM offset is not a general mapping for arbitrary globals in ROM.
Invalid logical setter ranges are ignored before any physical write.

There is a current LLVM exception at scale 4: partial values 8–15 have no outlined
setter and may still be stored inline at a zero poisoning threshold. The supplied
16-byte layout keeps logical and physical shadow identical, so those stores land
in DTCM, but bypass runtime range validation. The split eight-byte layout follows
the fully outlined contract. Fake-stack retirement also needs the zero threshold
so LLVM calls `__asan_stack_free_*` instead of using its inline saved-flag protocol.

### ROM shadow: compute bytes in LLVM, place them in LLD

The compiler knows each constant global's payload and redzone sizes, but the
linker decides its final position and the padding between globals. The pipeline
therefore separates byte computation from final placement:

```text
┌─────────────────────────────────────────────────────────────────────┐
│ C/C++ → LLVM IR → ASan global instrumentation                       │
│ Add global redzones; emit asan_globals descriptors.                 │
└───────────────────────────────┬─────────────────────────────────────┘
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ LLVM static-shadow computation: createGlobalShadow                  │
│ For constant, non-dynamically-initialized globals:                  │
│ payload → 00 bytes, partial tail → prefix count, redzones → f9.     │
└───────────────────────────────┬─────────────────────────────────────┘
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ ELF input sections                                                  │
│ One global per section; its __shadow_ro bytes carry !associated     │
│ / SHF_LINK_ORDER linkage. Association survives LTO and section GC.  │
└───────────────────────────────┬─────────────────────────────────────┘
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ LLD ordering and address assignment                                 │
│ Order associated shadows with their globals. Place each at          │
│ global_output_offset >> scale, including alignment and script gaps. │
│ Recompute during layout, then validate sizes, alignment, and order. │
└───────────────────────────────┬─────────────────────────────────────┘
                                ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Firmware image: .rodata + .asan_shadow in ROM                       │
│ Runtime reads the precomputed bytes through Platform::rom_shadow(). │
│ Startup initializes SRAM global shadow separately from descriptors. │
└─────────────────────────────────────────────────────────────────────┘
```

For an address inside the covered ROM output section:

```text
physical_shadow = __asan_ro_shadow_start
                + ((address - __asan_rodata_start) >> scale)
```

This mapping is independent of RAM's logical offset. The example linker script
places ROM shadow at `0x020f0000`, above the application constants, consuming no
DTCM or SRAM. Mutable runtime mappings exclude ROM shadow.

LLD fills scaled gaps between globals and at the section boundaries; the default
fill is zero. Uninstrumented data in the covered section consequently has
addressable shadow without object-specific redzones. Keep `.text` separate from
`.rodata`, use one instrumented global per input section (`-fdata-sections`), and
match compiler, linker, and runtime scales. LLD diagnoses incompatible ordering,
shared global sections, or mismatched shadow sizes. The supported final image is
a static ELF executable.

Eligibility is decided from LLVM IR when ASan runs. LLD does not promote globals
to read-only storage or route them between RAM and ROM; the linker script chooses
storage. Implementation details are in the [LLVM ASan pass](llvm/lib/Transforms/Instrumentation/AddressSanitizer.cpp)
and [LLD's static-shadow contract](lld/docs/ELF/asan_shadow.md).

## Hands-on: integrate a firmware

The following uses RA8M2's default eight-byte split layout as a worked example.
The same integration steps apply to a custom platform: choose memory bounds,
reserve shadow, supply startup and target hooks, and match the compiler flags.
The companion [asan-testbed](https://github.com/schrodingerzhu/asan-testbed)
provides complete RA8M2 startup, linker scripts,
and examples, including SRAM/ROM globals, stack lifetime errors, bulk operations,
and Mbed TLS.

### Define the platform and export the ABI

A target static-library crate depends on `baremetal-asan-rt`, implements
[`Platform`](baremetal-asan-rt/src/platform.rs), and invokes `export_asan!` once.
For example, the essential default-layout implementation is:

```rust
#![no_std]
use baremetal_asan_rt::platform::{Platform, Shadow, map_region};
use core::{ops::Range, ptr::addr_of};

// Required by this example's linker script. Take their addresses, not contents.
unsafe extern "C" {
    static __stack: u8;
    static __asan_alloc_base: u8;
    static __asan_alloc_size: u8;
    static __asan_rodata_start: u8;
    static __asan_rodata_end: u8;
    static __asan_ro_shadow_start: u8;
    static __asan_ro_shadow_end: u8;
}

struct Board;
impl Platform for Board {
    const APPLICATION: Range<usize> = 0x2200_0000..0x2218_c000;
    const SHADOW_SCALE: u32 = 3;
    const SHADOW_BASE: usize = 0x2000_0000;

    fn stack_top() -> usize { addr_of!(__stack) as usize }
    fn alloc_base() -> usize { addr_of!(__asan_alloc_base) as usize }
    fn alloc_size() -> usize { addr_of!(__asan_alloc_size) as usize }

    fn to_writable_shadow_ranges(addr: usize, size: usize)
        -> impl Iterator<Item = Shadow<Range<usize>>>
    {
        let last = size.checked_sub(1).and_then(|n| addr.checked_add(n));
        map_region::<Self>(addr, last, 0x2200_0000..0x2210_0000, 0x2000_0000)
            .into_iter()
            .chain(map_region::<Self>(
                addr, last, 0x2210_0000..0x2218_c000, 0x2218_c000,
            ))
    }

    fn rom_shadow() -> Option<Shadow<Range<usize>>> {
        // These bounds must satisfy the alignment/size contract below.
        let memory =
            addr_of!(__asan_rodata_start) as usize..addr_of!(__asan_rodata_end) as usize;
        let bytes =
            addr_of!(__asan_ro_shadow_start) as usize..addr_of!(__asan_ro_shadow_end) as usize;
        if memory.is_empty() || bytes.is_empty() { return None; }
        Some(Shadow { memory, bytes })
    }
}

baremetal_asan_rt::export_asan!(Board);
```

The example [RA8x2 target crate](ra8x2-asan-rt/src/lib.rs) supplies this implementation,
a semihosting panic handler, and `cortex-m`'s `critical-section-single-core`
backend. Its linker hooks use weak symbols and validate optional ROM bounds.
For a different target, supply a panic handler and critical-section backend in
that target crate and set its Cargo `crate-type` to `staticlib`.

The trait defaults to a single writable shadow region and no ROM mapping. For an
RP2040 or RP2350 port with shadow reserved in one SRAM region, set the application
bounds, scale, shadow base, and target hooks; the default writable mapping suffices.
Add `rom_shadow()` for precomputed flash shadow, and override the writable mapping
only when physical RAM shadow is split. Application
and shadow-covered region boundaries must be granule-aligned. Shadow storage,
program data, the allocation arena, and the real stack must have disjoint physical
storage. Reserve the arena inside `APPLICATION`; allocation initializes lazily
after shadow startup. An absent/zero arena disables heap and fake-stack allocation.

### Compute LLVM's shadow offset

`SHADOW_BASE` is the logical shadow address for `APPLICATION.start`. The trait
computes the offset with pointer-width wrapping subtraction:

```text
offset = SHADOW_BASE - (APPLICATION.start >> SHADOW_SCALE)
       = 0x20000000  - (0x22000000 >> 3)
       = 0x1bc00000

(0x22000000 >> 3) + 0x1bc00000 = 0x20000000
(0x22100000 >> 3) + 0x1bc00000 = 0x20020000 → physical 0x2218c000
```

`0x1bc00000` is an arithmetic offset, not the address of a reserved shadow buffer.
Pass scale `3` and that offset to LLVM; pass scale `3` to LLD as well. The preceding
layout table gives the matching values for the two other platforms.

### Reserve sections and define the linker contract

This script shows the default layout and a 32 KiB heap/fake-stack arena. The
allocation size symbol is an **absolute byte count**, not a variable in RAM.
The stack grows downward from `__stack`; the assertion leaves at least 16 KiB.

```ld
ENTRY(_start)
MEMORY {
    FLASH (rx)        : ORIGIN = 0x02000000, LENGTH = 960K
    SHADOW_ROM (r)    : ORIGIN = 0x020f0000, LENGTH = 64K
    RAM (rwx)        : ORIGIN = 0x22000000, LENGTH = 0x18c000
    SHADOW_DTCM (rw) : ORIGIN = 0x20000000, LENGTH = 128K
    SHADOW_SRAM (rw) : ORIGIN = 0x2218c000, LENGTH = 80K
}
SECTIONS {
    .vectors : { KEEP(*(.vectors)) } > FLASH
    .text : { *(.text*) } > FLASH
    .rodata : ALIGN(8) {
        __asan_rodata_start = .;
        *(.rodata*)
        . = ALIGN(8);
        __asan_rodata_end = .;
    } > FLASH
    .ARM.exidx : { *(.ARM.exidx*) } > FLASH
    .init_array : ALIGN(4) {
        __init_array_start = .;
        KEEP(*(SORT_BY_INIT_PRIORITY(.init_array.*))) KEEP(*(.init_array))
        __init_array_end = .;
    } > FLASH
    asan_globals : ALIGN(4) {
        __start_asan_globals = .;
        KEEP(*(asan_globals))
        __stop_asan_globals = .;
    } > FLASH
    .asan_shadow : {
        __asan_ro_shadow_start = .;
        *(__shadow_ro)
        __asan_ro_shadow_end = .;
    } > SHADOW_ROM
    .data : ALIGN(8) {
        __data_start = .; *(.data*) . = ALIGN(8); __data_end = .;
    } > RAM AT> FLASH
    __data_source = LOADADDR(.data);
    .bss (NOLOAD) : ALIGN(8) {
        __bss_start = .; *(.bss*) *(COMMON) . = ALIGN(8); __bss_end = .;
    } > RAM
    .asan_heap (NOLOAD) : ALIGN(64) {
        __asan_alloc_base = .;
        . += 32K;
        __asan_alloc_end = .;
    } > RAM
    __asan_alloc_size = SIZEOF(.asan_heap);
    __stack = ORIGIN(RAM) + LENGTH(RAM);
    ASSERT(__asan_alloc_end <= __stack - 16K, "heap overlaps stack")
    __shadow_dtcm_start = ORIGIN(SHADOW_DTCM);
    __shadow_dtcm_end = ORIGIN(SHADOW_DTCM) + LENGTH(SHADOW_DTCM);
    __shadow_sram_start = ORIGIN(SHADOW_SRAM);
    __shadow_sram_end = ORIGIN(SHADOW_SRAM) + LENGTH(SHADOW_SRAM);
    /DISCARD/ : { *(.comment) *(.note*) }
}
```

| Section / symbol | Consumer and requirement |
| --- | --- |
| `.vectors`, `__stack` | Cortex-M reset setup; `__stack` is also the runtime's exclusive upper bound for no-return cleanup. The supplied weak hook skips cleanup if it is absent/zero. |
| `.data`, `.bss`, `__data_*`, `__bss_*` | Firmware startup copies initialized data from its load address and zeros BSS. |
| `.init_array`, `__init_array_start/end` | Startup runs constructors after ASan initialization. |
| `asan_globals`, `__start_asan_globals`, `__stop_asan_globals` | Contiguous LLVM global descriptors; retain them and pass exclusive bounds to `__asan_init_globals`. |
| `.rodata`, `__asan_rodata_start/end` | Granule-aligned application ROM range covered by static shadow. |
| `.asan_shadow`, `__asan_ro_shadow_start/end` | Only `__shadow_ro` inputs; shadow length must equal the covered ROM length divided by the granule. |
| `.asan_heap`, `__asan_alloc_base`, `__asan_alloc_size` | Reserved arena for allocator state, live allocations, and quarantine. |
| `__shadow_dtcm_start/end`, `__shadow_sram_start/end` | Startup's RAM-clearing bounds; these names belong to the firmware, not the portable runtime ABI. |

Keep physical shadow outside the linker memory region available to ordinary data
and stack allocations. For other layouts, change both the script reservations
and the selected platform. The four ROM bounds are optional in the supplied
RA8x2 crate; missing, empty, or inconsistent bounds disable ROM checking.

### Initialize before instrumented code

The reset path must initialize `.data`/`.bss`, bring up the clock and memory
banks, initialize RAM shadow, initialize SRAM globals, then run constructors and
`main`. Supply an idempotent, **uninstrumented** `__asan_init`: LLVM's module
constructors also call it.

For the default RA8M2 layout, enable DTCM and clear shadow with full 64-bit writes
before byte access so its ECC state is initialized. Then call:

```c
extern unsigned char __start_asan_globals[], __stop_asan_globals[];
extern void __asan_init_globals(const void *, const void *);

/* Inside __asan_init, after clearing RAM shadow, once per startup: */
__asan_init_globals(__start_asan_globals, __stop_asan_globals);
```

Do not clear ROM shadow. Enable the required FP/MVE state before code using it.
The testbed starts at 1 GHz, enables both caches, and uses write-through SRAM so
semihosting buffers remain visible to the probe. An RTOS port must provide the
current task/handler stack bound through `stack_top()` rather than assuming one
fixed `__stack` works for every stack.

### Compile, link, and run

Build the patched tools and default target archive from this repository:

```sh
nix build .#llvm --out-link llvm-tools
nix build .#ra8x2-asan-rt --out-link asan-runtime
```

The outputs contain `llvm-tools/bin/clang`, `llvm-tools/bin/ld.lld`, and
`asan-runtime/lib/libra8x2_asan_rt.a`. The repository development shell also
supports Cargo builds for `granule-16`, `no-dtcm`, and `mve`; see the
[RA8x2 build commands](ra8x2-asan-rt/README.md).

For a small ROM overflow, use this `app.c`. Volatile accesses keep the compiler
from folding away the test; index 16 is valid and index 17 hits the partial granule.

```c
/* app.c */
static const unsigned char rom_data[17] = {1};
static volatile unsigned index = 17;

int main(void) {
    return ((volatile const unsigned char *)rom_data)[index];
}
```

Compile application translation units with the patched Clang and matching flags:

```sh
llvm-tools/bin/clang --target=armv8.1m.main-none-eabi -mcpu=cortex-m85 -mthumb -mfloat-abi=hard \
  -Os -g -ffreestanding -ffunction-sections -fdata-sections \
  -fsanitize=address -fsanitize-address-outline-instrumentation \
  -fsanitize-address-use-after-return=always -fsanitize-address-use-after-scope \
  -mllvm -asan-max-inline-poisoning-size=0 \
  -mllvm -asan-mapping-scale=3 -mllvm -asan-mapping-offset=0x1bc00000 \
  -mllvm -asan-globals-metadata-section=asan_globals \
  -mllvm -asan-create-global-shadow -c app.c -o app.o
```

The testbed exposes the same patched tool through `$CLANG`. Use
`-fsanitize-address-use-after-return=never` for a first example without fake stacks.
Compile startup, semihosting support, allocator adapters, and runtime dependencies
without ASan instrumentation. The allocator's C++ support is built directly by
Cargo's `cc` dependency, without a C++ standard library. It uses native objects
with C++ LTO disabled; Rust ThinLTO remains enabled.

The explicit final link has this shape, with board startup objects and target
libc/builtins supplied by the firmware build:

```sh
llvm-tools/bin/clang --target=armv8.1m.main-none-eabi -mcpu=cortex-m85 \
  -mthumb -mfloat-abi=hard -nostdlib --ld-path=llvm-tools/bin/ld.lld \
  -Wl,-T,ra8m2.ld -Wl,--gc-sections,--target2=rel -Wl,-Map,app.map \
  -Wl,--asan-shadow-section=.rodata:.asan_shadow,--asan-shadow-scale=3 \
  start.o platform.o semihost.o app.o asan-runtime/lib/libra8x2_asan_rt.a \
  "$TARGET_LIBC/libc.a" "$TARGET_LIBC/libclang_rt.builtins.a" -o app.elf

probe-rs run --chip R7KA8M2AF --non-interactive app.elf
```

`TARGET_LIBC` names the selected target multilib directory. The testbed pins ATFE
and chooses its `armv8.1m.main_hard_fpdp_nomve_exn_rtti_unaligned_size` multilib.
If a program uses libc's allocator as well, reserve its arena separately and
supply its `_end` / `__llvm_libc_heap_limit` contract. The ASan arena is reserved
for the runtime's explicit allocation entrypoints.

### Caveat: bulk memory operations under freestanding compilation

ASan rewrites LLVM's `memcpy`, `memmove`, and `memset` intrinsics to checked runtime
calls. With `-ffreestanding`, ordinary C calls to these functions can remain
external libc calls instead of becoming intrinsics. Instrumenting the caller
then leaves the memory accessed inside an uninstrumented libc unchecked.

Force-include a scoped header in the application and library sources being
sanitized. This is the approach used for Mbed TLS in the testbed:

```c
/* memory_wrap.h */
#ifndef ASAN_MEMORY_WRAP_H
#define ASAN_MEMORY_WRAP_H
#include <stddef.h>
#include <string.h> /* Parse libc declarations before defining redirects. */

#ifdef __cplusplus
extern "C" {
#endif
void *__asan_memcpy(void *dst, const void *src, size_t size);
void *__asan_memmove(void *dst, const void *src, size_t size);
void *__asan_memset(void *dst, int value, size_t size);
#ifdef __cplusplus
}
#endif

#define memcpy  __asan_memcpy
#define memmove __asan_memmove
#define memset  __asan_memset
#endif
```

Add `-include memory_wrap.h` to the application compile command above, using the
target libc's headers. For a CMake C application target named `app`:

```cmake
target_compile_options(app PRIVATE
  "SHELL:-include '${CMAKE_CURRENT_SOURCE_DIR}/memory_wrap.h'")
```

Apply this option separately to each source target being sanitized, including
third-party libraries. The object-like macros redirect both calls and function
references in those translation units; already compiled libraries are unaffected.
The wrappers check the complete source/destination ranges before calling the raw
memory operation (`memset` checks only its destination).

Keep startup, the ASan runtime, the allocator adapter, and raw libc outside this
wrapping scope. The wrappers themselves eventually use raw memory operations;
globally redirecting those symbols, for example with linker `--wrap`, can recurse
back into ASan. This header covers these three C memory APIs; route application
allocation through `__asan_malloc` / `__asan_free` with separate adapters.

### Run the complete testbed

For the complete CMake-driven examples, clone
[asan-testbed](https://github.com/schrodingerzhu/asan-testbed):

```sh
git clone https://github.com/schrodingerzhu/asan-testbed.git
cd asan-testbed
nix develop
cmake --preset default --fresh
cmake --build build --target check-global-in-bounds check-global-rom-oob
cmake --build build --target check-stack-uar check-bulk
cmake --build build --target check-mbedtls-baseline check-mbedtls-asan
# Or build and run every example:
cmake --build build --target check
```

`check-<case>` validates successful runs or expected ASan failures.
`run-<case>` streams the raw probe-rs output; an ASan abort returns failure.
Completed runs save stdout/stderr in `build/<case>.log`; `less -R` preserves the
colored shadow table. After changing the Git pin, re-enter the Nix shell and
configure with `--fresh` so CMake discards cached compiler/runtime paths.

## Space overhead

These measurements use Mbed TLS 3.6.7 on RA8M2 with the same patched compiler and
`-Os` for baseline and ASan. They are the recorded measurement snapshot, rather
than a size guarantee for later compiler or testbed revisions.

Static flash grows from **108.22 KiB to 234.84 KiB (+117%)**.

| Static contribution | Added flash |
| --- | ---: |
| Access-check instrumentation | 25.62 KiB |
| Stack instrumentation | 44.46 KiB |
| Runtime code and Rust helpers | 26.00 KiB |
| Stack-frame descriptions | 5.54 KiB |
| Global redzones, metadata, ROM shadow and associated data | 23.37 KiB |
| Other data, startup and alignment, net | 1.63 KiB |
| **Total** | **126.63 KiB** |

The footprint comparison uses a **256 KiB arena**, which passed all three TLS
rounds without allocation failures or fake-stack fallbacks.

| RAM footprint | Baseline | ASan | Increase |
| --- | ---: | ---: | ---: |
| Static RAM, including alignment gaps | 8.13 KiB | 10.29 KiB | +2.16 KiB |
| Reserved writable shadow | 0 | 208.00 KiB | +208.00 KiB |
| Peak application heap payload | 12.34 KiB | 12.34 KiB | 0 |
| Peak fake-stack storage | 0 | 20.94 KiB | +20.94 KiB |
| Peak live heap + fake-stack blocks, including block overhead | Unmeasured | 32.05 KiB | Unmeasured |
| Peak quarantine | 0 | 165.58 KiB | +165.58 KiB |
| Real-stack watermark | 11.30 KiB | 2.35 KiB | −8.95 KiB |

Rows overlap and peak at different times, so they cannot be summed. Stack
watermarks include observer frames; baseline allocator block overhead was not
measured. The regular Mbed TLS testbed still reserves 512 KiB; the 256 KiB
configuration was measured in a separate scratch build.

## Extensions and TODO

**Rotational quarantine heap from LLVM libc.** One arena contains two free-store
indices: reusable blocks and quarantined blocks. Frees enter quarantine; when
the active store cannot satisfy an allocation, rotation makes retired storage
available and coalesces free blocks. Live allocations stay in place. The runtime
shares this allocator between heap objects and fake-stack classes of
`64 << class` bytes (64 bytes through 64 KiB). Embassy's critical-section mutex
covers allocation, rotation, and associated shadow updates. See the
[allocator integration](baremetal-asan-rt/src/heap/allocator.cpp) and
[LLVM libc implementation](libc/src/__support/freelist_heap.h).

**MVE scanning for large shadow ranges.** The optional `mve` Cargo feature enables
an inline-assembly scanner using predicates to find invalid shadow bytes. In
thread mode, slices of at least 16 shadow bytes can use MVE; handlers and short
slices keep the scalar path. Fixed-size checks retain their small scalar scans.
Startup must enable CP10/CP11 and set `FPSCR.LEN` to `0b100`. MVE/FP context costs
still apply; see [the scanner](baremetal-asan-rt/src/access/scan.rs).

**TODO:** modularize fake-stack support and the other runtime features so firmware
can select the ABI groups, dependencies, and storage costs it needs. Export macros
can already select individual groups, but Cargo feature-level separation remains
to be done.
