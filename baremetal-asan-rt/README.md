# Portable bare-metal ASan runtime

The repository root is a Cargo workspace. `baremetal-asan-rt` is a `no_std` Rust
library containing the generic runtime and C ABI export macros. It has no device
addresses, linker-symbol dependency, semihosting dependency, or panic handler.
`ra8x2-asan-rt` is the target static library; see its [build and platform settings](../ra8x2-asan-rt/README.md).

A target crate implements `platform::Platform` and exports the ABI once:

```rust
use baremetal_asan_rt::platform::Platform;
use core::ops::Range;

struct MyPlatform;
impl Platform for MyPlatform {
    const APPLICATION: Range<usize> = 0x2000_0000..0x2001_0000;
    const SHADOW_SCALE: u32 = 3;
    const SHADOW_BASE: usize = 0x1000_0000;

    fn stack_top() -> usize {
        0x2001_0000
    }

    fn alloc_base() -> usize {
        0x2000_8000
    }

    fn alloc_size() -> usize {
        0x4000
    }
}

baremetal_asan_rt::export_asan!(MyPlatform);
```

`stack_top()` returns the current stack's exclusive upper bound; its default zero
skips no-return cleanup. `alloc_base()` and `alloc_size()` describe one reserved
arena for future heap and fake-stack allocations. They default to zero (no arena).
Reserve that arena within application RAM, disjoint from program data, the real
stack, and shadow. The example reserves `0x2000_8000..0x2000_c000` for allocation.

For split shadow, override `Platform::to_ranges`; each returned `Shadow` describes
an application range and its physical shadow range. `platform::map_region` handles
clipping and granule rounding. The default platform implementation uses one linear
shadow range. `to_slices` and `to_slices_mut` borrow those ranges without copying.
Shadow setters validate a logical shadow request before filling its physical
pieces. Target startup must reserve and initialize shadow RAM, and LLVM's mapping
scale and offset must match the platform.

`export_asan!` combines `export_asan_abi!`, `export_asan_globals!`,
`export_asan_access!`, `export_asan_memory!`, and `export_asan_stack!`. The group
macros can also be invoked individually. Merely linking this library emits no
ASan C symbols. Generic Rust helpers are available in `access`, `memory`, and
`stack`; their unsafe contracts cover shadow access and memory validity.

Fixed 1/2/4/8/16-byte checks borrow small shadow arrays and retain the expanded
scalar scans. Variable-size checks scan slices. The optional `mve` feature enables
the exact MVE slice scanner on bare-metal ARM; the consuming target must support
MVE and enable it at startup. Handlers and short slices retain scalar checks.

The existing ABI scope is unchanged: global registration, fake-stack allocation,
and lifetime/dynamic-stack hooks remain stubs. `__asan_init` is not implemented.
See [spec.md](spec.md) for the API list.

```sh
cargo build                      # Default workspace member: the portable library.
cargo test --workspace
cargo test -p baremetal-asan-rt --all-features
```

Release builds use `opt-level = "s"`, ThinLTO, and aborting panics. The consuming
runtime supplies its own panic handler. Access diagnostics include an ANSI-colored
shadow dump with the offending byte bracketed and a poison-value legend. Rows
show application addresses and remain continuous across physical shadow regions;
the dump is clipped to the platform's application bounds and uses its granule size.
