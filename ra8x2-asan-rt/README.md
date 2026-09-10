# RA8x2 ASan runtime

This `no_std` static library instantiates `baremetal-asan-rt` with an RA8M2
platform. It owns semihosting and its panic handler, plus optional weak linker
symbols for stack, allocation, and ROM bounds. `src/platform.rs` contains all device
addresses and the application-to-logical-to-physical shadow diagram.

| Feature | Platform | Application SRAM (exclusive end) | Physical shadow |
| --- | --- | --- | --- |
| Default | `Ra8m2Granule8` | `0x2200_0000..0x2218_c000` (1584 KiB) | 128 KiB at `0x2000_0000`, then 70 KiB at `0x2218_c000` |
| `granule-16` | `Ra8m2Granule16` | `0x2200_0000..0x221a_0000` (1664 KiB) | 104 KiB at `0x2000_0000` |
| `no-dtcm` | `Ra8m2Granule8Sram` | `0x2200_0000..0x2217_0000` (1472 KiB) | 184 KiB at `0x2217_0000`; DTCM is unused |

The default platform reserves the last 80 KiB of main SRAM; `no-dtcm` reserves the
last 192 KiB. The linker must exclude these reservations from application
allocations. Initialize all used shadow bytes before instrumented code runs.
These standalone layouts do not sanitize application data in TCM, and firmware
with other RAM reservations needs a matching platform and linker script.

After clearing shadow, call `__asan_init_globals(__start_asan_globals,
__stop_asan_globals)` before instrumented constructors. Keep the `asan_globals`
descriptor section and define these exclusive bounds in the linker script. The
runtime initializes SRAM payload shadow and poisons global redzones with `0xf9`,
including partial final granules; descriptors for flash constants are skipped.
A compiler built with the imported LLVM PR #212890 can emit this section with
`-mllvm -asan-globals-metadata-section=asan_globals`. The normal ELF metadata
section has the same ABI and also supports explicit startup initialization.

All three platforms also support precomputed ROM shadow through four weak linker
symbols:

| Application ROM | Physical ROM shadow |
| --- | --- |
| `__asan_rodata_start..__asan_rodata_end` | `__asan_ro_shadow_start..__asan_ro_shadow_end` |

Range ends are exclusive. The application bounds must be granule-aligned and the
shadow size must equal the application size divided by the selected granule.
Missing, empty, or inconsistent bounds disable this optional mapping. Physical
ROM shadow can be placed at a higher ROM address; it uses no TCM or SRAM.

Compile constant globals with `-fdata-sections`,
`-mllvm -asan-create-global-shadow`, and the metadata-section option above. Keep
`.rodata` separate from `.text`, collect `__shadow_ro` into `.asan_shadow`, and
link with `--asan-shadow-section=.rodata:.asan_shadow` and
`--asan-shadow-scale=3` (or `4` for `granule-16`). Define the four symbols at the
bounds of those output sections. The [static-shadow linker documentation](../lld/docs/ELF/asan_shadow.md)
describes the full contract. The runtime reads ROM shadow directly; SRAM startup
initialization and mutable shadow hooks leave it untouched. Access reports also
display ROM shadow bytes.

Build the target archive (`target/thumbv8m.main-none-eabihf/release/libra8x2_asan_rt.a`):

```sh
cargo build -p ra8x2-asan-rt --release --target thumbv8m.main-none-eabihf
cargo build -p ra8x2-asan-rt --release --target thumbv8m.main-none-eabihf --features granule-16
cargo build -p ra8x2-asan-rt --release --target thumbv8m.main-none-eabihf --features no-dtcm
```

`granule-16` and `no-dtcm` are mutually exclusive. Any platform can additionally use
`mve` on Cortex-M85:

```sh
cargo rustc -p ra8x2-asan-rt --release --features mve,no-dtcm \
  --target thumbv8m.main-none-eabihf -- -C target-cpu=cortex-m85
```

Thread-mode slices of at least 16 shadow bytes use MVE; shorter slices and handlers
use scalar checks. Startup must enable CP10/CP11 access and set FPSCR.LEN to
`0b100`. The predicate and address stride follow the selected granule size.

LLVM computes logical shadow addresses. The runtime translates them into the
physical slices above. Compile instrumented application code with:

```sh
-fsanitize=address -fsanitize-address-outline-instrumentation \
-fsanitize-address-use-after-return=runtime \
-mllvm -asan-max-inline-poisoning-size=0
```

Also supply the mapping options matching the runtime:

| Feature | `-mllvm -asan-mapping-scale=` | `-mllvm -asan-mapping-offset=` |
| --- | --- | --- |
| Default | `3` | `0x1bc00000` |
| `granule-16` | `4` | `0x1de00000` |
| `no-dtcm` | `3` | `0x1dd70000` |

Fake-stack detection defaults to enabled when an arena is available. `always`
also works; `never` keeps the real stack. Keep the poisoning threshold at zero
so small fake frames are returned through `__asan_stack_free_*` calls.

Inline shadow accesses bypass translation and cannot be used with the split
platform. LLVM has no outlined setters for partial-shadow values 8–15: at scale 4,
those writes remain inline even with the poisoning threshold set to zero. They
address the correct DTCM bytes but bypass the setters' range guards. The
`no-dtcm` platform also has identical logical and physical shadow addresses.

`__asan_handle_no_return` clears the current downward-growing stack's shadow up
to the optional weak `__stack` symbol. An undefined or zero symbol skips cleanup.

`Platform::alloc_base()` and `alloc_size()` read the values of the weak linker
symbols `__asan_alloc_base` and `__asan_alloc_size`. The base is an application
address and the size is a byte count. Undefined symbols return zero. To reserve
an allocation arena after static data and below a 16 KiB stack, a linker script
can define:

```ld
__asan_alloc_base = ALIGN(__bss_end, 16);
__asan_alloc_size = (__stack - 16K) - __asan_alloc_base;
ASSERT(__asan_alloc_base <= __stack - 16K, "allocation arena overlaps stack")
```

The linker must keep other sections out of this arena. `baremetal_asan_rt::heap::Heap`
uses it for LLVM libc's allocator state and allocation storage. This target
supplies the Cortex-M single-core critical-section backend. `__asan_malloc`,
`__asan_free`, and fake-stack calls share one heap, initialize it lazily, and
manage their shadow under the same mutex. Startup must initialize shadow first.
Frames bypassed by nonlocal returns are not reclaimed yet.

```sh
cargo test --workspace
cargo test -p ra8x2-asan-rt --features granule-16
cargo test -p ra8x2-asan-rt --features no-dtcm
```
