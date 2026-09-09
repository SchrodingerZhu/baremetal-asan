# Bare-metal ASan runtime

Build the scalar static library for Cortex-M:

```sh
cargo build -p baremetal-asan-rt --release --target thumbv8m.main-none-eabihf
```

All checks share the same scalar poison rule and boundary handling. Fixed
1/2/4/8/16-byte checks borrow arrays of one to three shadow bytes. `ShadowScan`
provides separate array implementations that expand into short-circuit checks;
variable-size accesses use the slice iterator. This keeps fixed scans unrolled
at `opt-level = "s"` without copying shadow memory.

Startup must initialize shadow RAM before instrumented code runs. The prototype
RAM/shadow mapping in `src/access.rs` must match the device. Release builds use
`opt-level = "s"` and ThinLTO from the workspace manifest.

Run the host mapping, boundary, and poisoning tests with:

```sh
cargo test -p baremetal-asan-rt
```
