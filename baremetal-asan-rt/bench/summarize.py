#!/usr/bin/env python3
"""Extract benchmark CSV and summarize measured crossover points."""
import csv
import sys
from pathlib import Path

log = Path(sys.argv[1]).read_text()
if "VALIDATION,passed" not in log or "\nDONE\n" not in log:
    raise SystemExit("incomplete or failed benchmark run")
header = ["kind", "implementation", "memory", "size", "offset", "cycles", "iterations"]
rows = [r for r in csv.reader(log.splitlines()) if len(r) == 7 and r[0] in
        ("overhead", "fixed", "scan", "range", "svc", "svc_range")]
output = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(sys.argv[1]).with_suffix(".csv")
with output.open("w", newline="") as f:
    writer = csv.writer(f)
    writer.writerow(header)
    writer.writerows(rows)

values = {(r[0], r[1], r[2], int(r[3]), int(r[4])): int(r[5]) / int(r[6]) for r in rows}
print("Fixed loads, aligned: size, baseline, specialized (cycles/call, harness included)")
for size in (1, 2, 4, 8, 16):
    print(size, *(round(values["fixed", impl, "dtcm", size, 0], 2)
                  for impl in ("baseline", "specialized")))

for kind in ("scan", "range", "svc_range"):
    for memory in sorted({r[2] for r in rows if r[0] == kind}):
        sizes = sorted({int(r[3]) for r in rows if r[0] == kind and r[2] == memory})
        offsets = sorted({int(r[4]) for r in rows if r[0] == kind and r[2] == memory})
        wins = []
        for size in sizes:
            wins.append(all(values[kind, "mve", memory, size, offset] < min(
                values[kind, impl, memory, size, offset]
                for impl in ("byte", "word", "word_unaligned")) for offset in offsets))
        stable = next((size for i, size in enumerate(sizes) if all(wins[i:])), None)
        unit = "shadow bytes" if kind == "scan" else "application bytes"
        print(f"{kind}/{memory}: MVE strictly wins at every sampled size/alignment from {stable} {unit}")
print(f"Saved {len(rows)} measurements to {output}")
