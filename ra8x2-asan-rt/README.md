# RA8x2 ASan runtime

This `no_std` static library instantiates `baremetal-asan-rt` with an RA8M2 memory
layout. It owns semihosting and its panic handler, plus the optional weak
`__stack` lookup. `src/layout.rs` contains all device addresses and the ASCII
application-to-logical-to-physical shadow diagram.

| Feature | Layout | Application SRAM (exclusive end) | Physical shadow |
| --- | --- | --- | --- |
| Default | `Ra8m2Granule8` | `0x2200_0000..0x2218_c000` (1584 KiB) | 128 KiB at `0x2000_0000`, then 70 KiB at `0x2218_c000` |
| `granule-16` | `Ra8m2Granule16` | `0x2200_0000..0x221a_0000` (1664 KiB) | 104 KiB at `0x2000_0000` |
| `no-dtcm` | `Ra8m2Granule8Sram` | `0x2200_0000..0x2217_0000` (1472 KiB) | 184 KiB at `0x2217_0000`; DTCM is unused |

The default layout reserves the last 80 KiB of main SRAM; `no-dtcm` reserves the
last 192 KiB. The linker must exclude these reservations from application
allocations. Initialize all used shadow bytes before instrumented code runs.
These standalone layouts do not sanitize application data in TCM, and firmware
with other RAM reservations needs a matching layout and linker script.

Build the target archive (`target/thumbv8m.main-none-eabihf/release/libra8x2_asan_rt.a`):

```sh
cargo build -p ra8x2-asan-rt --release --target thumbv8m.main-none-eabihf
cargo build -p ra8x2-asan-rt --release --target thumbv8m.main-none-eabihf --features granule-16
cargo build -p ra8x2-asan-rt --release --target thumbv8m.main-none-eabihf --features no-dtcm
```

`granule-16` and `no-dtcm` are mutually exclusive. Any layout can additionally use
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
-fsanitize-address-use-after-return=never \
-mllvm -asan-max-inline-poisoning-size=0
```

Also supply the mapping options matching the runtime:

| Feature | `-mllvm -asan-mapping-scale=` | `-mllvm -asan-mapping-offset=` |
| --- | --- | --- |
| Default | `3` | `0x1bc00000` |
| `granule-16` | `4` | `0x1de00000` |
| `no-dtcm` | `3` | `0x1dd70000` |

Inline shadow accesses bypass translation and cannot be used with the split
layout. LLVM has no outlined setters for partial-shadow values 8–15: at scale 4,
those writes remain inline even with the poisoning threshold set to zero. They
address the correct DTCM bytes but bypass the setters' range guards. The
`no-dtcm` layout also has identical logical and physical shadow addresses.

`__asan_handle_no_return` clears the current downward-growing stack's shadow up
to the optional weak `__stack` symbol. An undefined or zero symbol skips cleanup.

```sh
cargo test --workspace
cargo test -p ra8x2-asan-rt --features granule-16
cargo test -p ra8x2-asan-rt --features no-dtcm
```
