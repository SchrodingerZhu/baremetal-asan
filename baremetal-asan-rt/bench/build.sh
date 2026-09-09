#!/usr/bin/env bash
set -euo pipefail
here=$(cd -- "$(dirname -- "$0")" && pwd)
root=$(cd -- "$here/../.." && pwd)
infra=${RA8XX_INFRA:-$root/../llvm-libc-ra8xx-test}
build=${BENCH_BUILD:-$root/target/ra8xx-bench}
rustc=${RUSTC:-rustc}
cargo=${CARGO:-cargo}
clang=${CLANG:-clang}
mkdir -p "$build"
flags=(--target=armv8.1m.main-none-eabi -mcpu=cortex-m85 -mfloat-abi=hard
       -mthumb -Os -ffreestanding -ffunction-sections -fdata-sections)
"$cargo" rustc --manifest-path "$root/Cargo.toml" -p baremetal-asan-rt --release \
    --target thumbv8m.main-none-eabihf --locked --offline -- \
    -C target-cpu=cortex-m85 -C opt-level=s
"$rustc" --edition=2024 --crate-type=lib --target thumbv8m.main-none-eabihf \
    -C target-cpu=cortex-m85 -C opt-level=s -C panic=abort --emit=obj \
    "$here/baseline-lib.rs" -o "$build/baseline.o"
"$clang" "${flags[@]}" -DRA8XX_CGC=2 -c "$infra/semihost.c" -o "$build/semihost.o"
for source in main.c scans.c start.S; do
    "$clang" "${flags[@]}" -c "$here/$source" -o "$build/${source%.*}.o"
done
"$clang" "${flags[@]}" -nostdlib -fuse-ld=lld -Wl,--gc-sections \
    -Wl,-T,"$here/bench.ld" -Wl,-Map,"$build/bench.map" \
    "$build/start.o" "$build/main.o" "$build/scans.o" "$build/semihost.o" "$build/baseline.o" \
    "$root/target/thumbv8m.main-none-eabihf/release/libbaremetal_asan_rt.a" \
    -o "$build/bench.elf"
echo "$build/bench.elf"
