# Bare-metal ASan runtime

Build the scalar static library for Cortex-M:

```sh
cargo build -p baremetal-asan-rt --release --target thumbv8m.main-none-eabihf
```

All checks share the same poison rule and boundary handling. Fixed
1/2/4/8/16-byte checks borrow arrays of one to three shadow bytes. `ShadowScan`
provides separate array implementations that expand into short-circuit checks;
variable-size accesses use the slice iterator by default. This keeps fixed scans
unrolled at `opt-level = "s"` without copying shadow memory.

Enable the exact MVE slice scan for thread mode on an MVE-capable target:

```sh
cargo rustc -p baremetal-asan-rt --release --features mve \
  --target thumbv8m.main-none-eabihf -- -C target-cpu=cortex-m85
```

Thread-mode slices of at least 16 shadow bytes use MVE, scanning 16 bytes per
iteration and returning the first invalid application address, including partial
granules. Shorter slices and handlers retain the scalar iterator; fixed accesses
retain their scalar array checks. Startup must enable CP10/CP11 access and set
FPSCR.LEN to `0b100` before using MVE.

Startup must initialize shadow RAM before instrumented code runs. The prototype
RAM/shadow mapping in `src/access.rs` must match the device. Release builds use
`opt-level = "s"` and ThinLTO from the workspace manifest.

The memory wrappers check their ranges before copying or filling bytes.
`__asan_handle_no_return` clears stack poisoning from its current frame up to the
optional weak linker symbol `__stack`, assuming one downward-growing stack.
If `__stack` is undefined or zero, cleanup is skipped. Otherwise it is clipped
to the configured application SRAM.

Run the host mapping, boundary, and poisoning tests with:

```sh
cargo test -p baremetal-asan-rt
```
