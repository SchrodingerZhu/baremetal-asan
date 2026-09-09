# RA8xx access-check benchmark

Measured on EK-RA8M2 / R7KA8M2AF, Cortex-M85, at **1 GHz** on 2026-09-09.
The benchmark reuses `../llvm-libc-ra8xx-test/semihost.c` for clock setup and
output, and its `run.sh` for probe-rs execution. Clock register values are checked
before timing. I-cache is on, D-cache is off, and unaligned word access is enabled.

## Fixed-size specialization

The runtime now expands the 1/2/4/8/16-byte checks into at most three shadow
reads, preserving partial-granule, unaligned, overflow, and boundary behavior.
Boundary cases tail-call the general checker. The ordinary 1-byte path has no
stack frame; the other fixed sizes save only `{r7, lr}` on this build.

Cycles per call, including the same indirect-call benchmark harness:

| Access bytes | Old loop | Specialized | Reduction |
| --- | ---: | ---: | ---: |
| 1 | 256 | 43 | 83% |
| 2 | 260 | 100 | 62% |
| 4 | 260 | 100 | 62% |
| 8 | 260 | 100 | 62% |
| 16 | 256 | 100 | 61% |

These are aligned accesses. Offset 7 gave the same specialized timings; the old
loop varied between 256 and 260 cycles. At 1 GHz, one cycle is one nanosecond.
The empty-call harness measured about 19 cycles/call; tables do not subtract it.

## Bulk crossover

The C candidates scan zero/nonzero shadow bytes using byte loads, aligned word
loads with a byte prefix/tail, unaligned word loads, or predicated MVE loads.
Full range wrappers clip the application range and fall back to the exact Rust
checker on nonzero shadow, so partial granules remain correct. These candidates
are benchmark code; the runtime's `N` entry points still use the general checker.

Raw DTCM scan, aligned shadow address, cycles per call:

| Shadow bytes | Application coverage | Best scalar | MVE |
| --- | --- | ---: | ---: |
| 8 | 64 bytes | 96 | 96 |
| 12 | 96 bytes | 96 | 96 |
| 16 | 128 bytes | 100 | 96 |
| 32 | 256 bytes | 132 | 96 |
| 64 | 512 bytes | 196 | 120 |

Across all tested shadow alignments (0, 1, 3, 15), MVE consistently wins from
**16 shadow bytes**, roughly 128 application bytes, through the largest tested
512-shadow-byte span. This is a conservative bulk threshold, with a small gain
at 16 bytes and a clearer margin at 32 bytes. In uncached SRAM the corresponding
sampled crossover is 6 shadow bytes; memory placement matters substantially.

The complete MVE range prototype already wins at the smallest tested 8-byte
application range (45 cycles versus 96 for the best scalar candidate and 252 for
the current Rust loop). It also avoids scalar register-save/setup costs, so this
is not a pure SIMD throughput comparison. The fixed-size Rust paths and the raw
scan crossover are the better guides for deciding when to introduce MVE state.

## Interrupt cost

An SVC invokes the checker with lazy FP preservation enabled. An interrupted
thread with active FP/MVE state makes the short MVE scan cost about **296 extra
cycles** compared with the same scan after an FP-inactive thread. Scalar scans
change by approximately one cycle. The empty SVC costs 394 cycles in either case.

Complete range checks inside an SVC interrupting FP-active code; total cycles
include exception entry/return and the harness:

| Application bytes | Best scalar candidate | MVE |
| --- | ---: | ---: |
| 128 | 482 | 733 |
| 512 | 590 | 769 |
| 1024 | 734 | 829 |
| 1536 | 878 | 889 |
| 2048 | 1010 | 949 |
| 4096 | 1522 | 1189 |

Use **2 KiB as the conservative sampled crossover when an ISR can trigger lazy
preservation**. Keep small checks scalar. This cost is paid when deferred context
preservation is triggered, not on every subsequent MVE call in the same handler.
The measured power-state register was zero; EPU retention wake-up is not included.

## Measurement details

- Rust 1.98.0 / LLVM 22.1.8, `-C target-cpu=cortex-m85 -C opt-level=s`.
  C and startup: Clang 23.1.0, Cortex-M85 hard-float, `-Os`, without LTO.
  Optimization flags are explicit because the member crate's Cargo release
  profile is ignored by the workspace.
- Ordinary timings: 32 warm-up calls, then five batches of 512 calls; reported
  value is the median batch. SVC timings: minimum of 65 single invocations with
  the interrupted FP state set before each invocation. No semihosting occurs
  in timed intervals. DWT measures target cycles, not host time.
- Measurements cover accessible, zero-shadow paths. Scan validation also checks
  every poison position for lengths 1..65 at offsets 0..15, including poison just
  outside the active span. Rust tests check every signed shadow value for all
  fixed sizes and boundary cases.
- The runtime currently assumes application addresses in `0x20040000..0x20200000`.
  On this board, `0x20000000..0x20020000` is DTCM and SRAM starts at `0x22000000`.
  The benchmark therefore uses synthetic application addresses and initializes
  the valid shadow slice at `0x20008000`; it never dereferences those application
  addresses. Separate raw scans use a real SRAM buffer. The full prototype RAM
  mapping is not a valid description of this board's memory map.
- [results.csv](results.csv) contains all 987 measurements from the final run.
  `baseline.rs` preserves the former loop for reproducible comparisons.

## Reproduce

Run from the repository root with a working Rust toolchain, its
`thumbv8m.main-none-eabihf` target, an unwrapped Clang with MVE headers, and probe-rs:

```sh
CLANG=/path/to/clang bash baremetal-asan-rt/bench/build.sh
RA8XX_TEST_TIMEOUT=150 ../llvm-libc-ra8xx-test/run.sh R7KA8M2AF \
  target/ra8xx-bench/bench.elf > target/ra8xx-bench/run.log 2>&1
python3 baremetal-asan-rt/bench/summarize.py target/ra8xx-bench/run.log
```

The run flashes the benchmark image. `RA8XX_INFRA`, `BENCH_BUILD`, `CARGO`,
`RUSTC`, and `CLANG` override the build script's defaults.

For the inner-loop scheduling model:

```sh
llvm-mca -mtriple=thumbv8.1m.main-none-eabi -mcpu=cortex-m85 \
  -iterations=100 baremetal-asan-rt/bench/mca.s
```

LLVM-MCA 23.1.0 models 100 byte/word/MVE loop iterations in 701/602/1102 cycles,
respectively, processing 1/4/16 shadow bytes per iteration. This supports the
bulk-vector advantage, but does not model the board's SRAM waits, exception
stacking, or semihosting. Hardware measurements determine the thresholds above.
