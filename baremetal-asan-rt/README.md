# Portable bare-metal ASan runtime

The repository root is a Cargo workspace. `baremetal-asan-rt` is a `no_std` Rust
library containing the generic runtime and C ABI export macros. It has no device
addresses, linker-symbol dependency, semihosting dependency, or panic handler.
`ra8x2-asan-rt` is the target static library; see its [build and platform settings](../ra8x2-asan-rt/README.md).

`build.rs` uses `cc` to compile the in-tree LLVM libc allocator support directly,
with dual-free-store rotation from [LLVM PR #209811](https://github.com/llvm/llvm-project/pull/209811)
and the 32-bit TLSF table fix from [PR #221807](https://github.com/llvm/llvm-project/pull/221807).
It bundles `freelist.cpp`, `freetrie.cpp`, and our arena FFI; `FreeListHeap`,
`BlockRef`, and `FreeStore` are header-only. This needs a C++17 compiler and archiver, without
a CMake build, generated libc headers, or a C++ standard library. For cross builds,
set `CXX=clang++ AR=llvm-ar` (the repository's Nix shell already provides them).
Native objects keep linkage independent of Rust's LLVM bitcode version; Rust
ThinLTO remains enabled. Allocator assertions are disabled because the standalone
build has no libc assertion/exit backend.

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
arena for heap and fake-stack allocations. They default to zero (no arena).
Reserve that arena within application RAM, disjoint from program data, the real
stack, and shadow. The example reserves `0x2000_8000..0x2000_c000` for allocation.

`heap::Heap<P>` initializes LLVM libc's heap in this arena. Its prefix holds the
C++ allocator state, so usable capacity is smaller than `alloc_size()`. Create a
heap with `Heap::new()`, call `unsafe { heap.init() }` once, then use
`heap.allocate(Layout)` and `unsafe { heap.deallocate(ptr) }`. Initialization
returns false for invalid/insufficient storage or an already initialized heap;
allocation returns `None` before initialization, for zero size, or on exhaustion.
An Embassy `CriticalSectionMutex` serializes every operation; RA8x2 supplies the
Cortex-M single-core backend. Frees enter quarantine. Allocation pressure rotates
the stores and coalesces remaining free blocks, holding the critical section for
the entire rotation. Live allocations stay in place.

The raw C interface is declared in [allocator.h](src/heap/allocator.h):

| Function | Functionality |
| --- | --- |
| `__baremetal_asan_heap_init(base, size)` | Construct allocator state inside the arena and return an opaque handle, or null on failure. |
| `__baremetal_asan_heap_allocate(heap, size, alignment)` | Allocate with power-of-two alignment; size need not be an alignment multiple. |
| `__baremetal_asan_heap_deallocate(heap, ptr)` | Quarantine a live allocation; a null pointer is ignored. |

C callers of this raw interface must initialize each arena once and serialize
access themselves. The build does not include libc's global heap or export its
`malloc`, `free`, `calloc`, `realloc`, or `aligned_alloc` entrypoints.

`export_asan!` shares one lazily initialized heap between `__asan_malloc`,
`__asan_free`, and fake-stack allocations. Malloc adds poisoned left/right redzones
and makes exactly the requested bytes accessible, including partial granules.
Free poisons the allocation with `0xfd` and returns it to LLVM libc's quarantine.
Zero-sized malloc returns a freeable, fully poisoned pointer; exhaustion or size
overflow returns null. These symbols are explicit entrypoints, not aliases for
application `malloc`/`free`.

Fake-stack classes 0–10 allocate 64 bytes through 64 KiB directly from the same
heap, aligned to their class size. LLVM writes the live frame's header/redzones;
`__asan_stack_free_*` poisons the entire returned frame with `0xf5` and releases
it to the existing quarantine. There is no additional frame quarantine. Runtime
use-after-return detection defaults to enabled; `_always` calls ignore its flag.
Exhaustion returns zero so LLVM can fall back to the real stack.

Fake-stack retirement requires `-mllvm -asan-max-inline-poisoning-size=0`: the
compiler must call `__asan_stack_free_*`, including for small frames, instead of
its inline saved-flag protocol. Frames whose return is bypassed by `longjmp` or
other nonlocal control flow are not reclaimed yet. The current no-return hook
only clears real-stack shadow.

Stack lifetime hooks poison ended objects with `0xf8` and restore their live
prefixes without invalidating neighboring bytes in a partial granule. Dynamic
`alloca`/VLA hooks apply LLVM's 32-byte left/right redzones (`0xca`/`0xcb`) and
clear retired dynamic-stack shadow on return or stack restore. These dynamic
allocations stay on the real stack; they do not gain fake-stack after-return
protection. All of these updates use the platform's physical shadow slices.

For split RAM shadow, override `Platform::to_writable_shadow_ranges`; each returned `Shadow` describes
an application range and its physical shadow range. `platform::map_region` handles
clipping and granule rounding. The default platform implementation uses one linear
shadow range. `to_shadow_slices` and `to_shadow_slices_mut` borrow those ranges without copying.
Shadow setters validate a logical shadow request before filling its physical
pieces. Target startup must reserve and initialize shadow RAM, and LLVM's mapping
scale and offset must match the platform.

`export_asan!` combines `export_asan_abi!`, `export_asan_globals!`,
`export_asan_access!`, `export_asan_memory!`, `export_asan_heap!`, and
`export_asan_stack!`. When exporting heap and stack groups individually, pass the
same `Heap<Platform>` static to `export_asan_heap!(Platform, HEAP)` and
`export_asan_stack!(Platform, HEAP)`. Merely linking this library emits no ASan C
symbols. Generic Rust helpers are available in `access`, `global`, `memory`, `heap`, and
`stack`; their unsafe contracts cover arena ownership, shadow, and memory validity.

Fixed 1/2/4/8/16-byte checks borrow small shadow arrays and retain the expanded
scalar scans. Variable-size checks scan slices. The optional `mve` feature enables
the exact MVE slice scanner on bare-metal ARM; the consuming target must support
MVE and enable it at startup. Handlers and short slices retain scalar checks.

After clearing shadow, startup calls `__asan_init_globals(start, end)` with the
exclusive bounds of a contiguous `__asan_global` descriptor section. It makes
SRAM global payloads accessible and poisons their right redzones with `0xf9`,
including the valid prefix of a partial final granule. Descriptors outside
`Platform::APPLICATION` (such as flash constants) are skipped. The Rust interface
is `global::init::<Platform>(&[Global])`; `export_asan_globals!(Platform)` emits
the C helper. Run this before instrumented constructors, with exclusive shadow
access. Startup's existing initialization guard should also cover this call.

The included [LLVM PR #212890](https://github.com/llvm/llvm-project/pull/212890)
adds the compiler option
`-mllvm -asan-globals-metadata-section=asan_globals`, emitting descriptors without
automatic registration calls. The normal ELF `asan_globals` section also uses
the same descriptor ABI and works with explicit startup initialization. Keep the
metadata section in the linker script and supply its start/end symbols.
Automatic registration/unregistration and initialization-order hooks remain stubbed.
`__asan_init` is not implemented; startup must initialize shadow before allocation.
See [spec.md](spec.md) for the API list.

`Platform::rom_shadow()` optionally supplies a ROM application range and the
physical range holding its precomputed shadow. `APPLICATION` continues to describe
RAM. Read mappings (`to_shadow_ranges` / `to_shadow_slices`) include both regions
in application-address order; `to_shadow_slices_mut` uses only writable RAM
mapping. Stack, heap, and global poisoning therefore never borrow ROM shadow
mutably. ROM placement is independent of RAM's logical shadow offset, and ordinary
RAM checks do not resolve ROM bounds. See the [compiler/linker contract](../lld/docs/ELF/asan_shadow.md)
for static shadow emission and placement.

```sh
cargo build                      # Default workspace member: the portable library.
cargo test --workspace
cargo test -p baremetal-asan-rt --all-features
```

Release builds use `opt-level = "s"`, ThinLTO, and aborting panics. The consuming
runtime supplies its own panic handler. Access diagnostics include an ANSI-colored
shadow dump with the offending byte bracketed and a poison-value legend. Rows
show application addresses and remain continuous across physical shadow regions;
the dump is clipped to the containing RAM or ROM application range and uses its
granule size.
