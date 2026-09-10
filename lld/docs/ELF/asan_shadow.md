# Static AddressSanitizer shadow for ROM globals

A bare-metal runtime can read precomputed shadow from ROM instead of reserving
RAM for constant-global shadow. LLVM's `-asan-create-global-shadow` emits one
constant byte array in `__shadow_ro` for each eligible constant global. These
arrays contain addressable bytes, partial-granule counts, and `0xf9` redzones.
They use `!associated` / `SHF_LINK_ORDER` to follow their globals through section
garbage collection. Mutable and dynamically initialized globals do not receive
static shadow. The existing global metadata section is still emitted for all
instrumented globals, allowing the runtime to initialize SRAM shadow separately.

For example, compile with:

```sh
clang --target=thumbv8m.main-none-eabi -fdata-sections -fsanitize=address \
  -fsanitize-address-outline-instrumentation \
  -mllvm -asan-create-global-shadow \
  -mllvm -asan-globals-metadata-section=asan_globals -c globals.c
```

The compiler option requires an ELF target and section metadata. Ordinary ASan
global eligibility rules still apply. Each instrumented constant global must
occupy its own input section; `-fdata-sections` supplies this for ordinary globals.
Explicit section attributes must also preserve that property. LLD diagnoses
shared input sections or mismatched scales instead of producing misplaced shadow.

## Placement

Link with `--asan-shadow-section=.globals:.shadow`. The names designate one pair
of output sections. `--asan-shadow-scale=3` selects eight-byte granules (the
default); use `4` with LLVM's `-asan-mapping-scale=4` for sixteen-byte granules.
For an application address in `.globals`, the runtime mapping is:

```text
ADDR(.shadow) + ((address - ADDR(.globals)) >> scale)
```

The linker script selects the physical address of both sections. For example,
these illustrative addresses put the shadow above its globals:

```text
SECTIONS {
  .globals 0x10000000 : {
    __asan_rodata_start = .;
    *(.rodata*)
    . = ALIGN(8);
    __asan_rodata_end = .;
  }
  .shadow 0x18000000 : {
    __asan_ro_shadow_start = .;
    *(__shadow_ro)
    __asan_ro_shadow_end = .;
  }
}
```

In a complete platform script, place both sections in actual ROM and collect
`asan_globals` metadata separately. The shadow output must contain only static
shadow input sections. Its size is the global output size divided by the granule,
rounded up, including shadow for leading, intervening, and trailing gaps. Gaps
use the script's fill pattern, zero by default. Uninstrumented data in `.globals`
therefore has addressable shadow rather than object-specific redzones. Keep the
covered output section bounded to avoid allocating shadow for unused address
windows or unrelated code.

LLD uses the associated global's final section offset, including alignment and
linker-script padding, to place each shadow array. Placement is recomputed during
address assignment. Explicit input-description ordering is respected; conflicting
ordering, alignment, or script contents are errors. Full LTO and ThinLTO preserve
the association. Relocatable links defer placement until the final link, and must
preserve one global per input section. PIE and shared objects are not supported.

Compiler optimization may mark an IR global constant. This is distinct from
ordinary linker relocations or `.data.rel.ro` becoming read-only at runtime.
LLD does not promote or reroute sections between ROM and RAM. Eligibility is
decided when ASan instruments the globals; the script defines their final storage.
The runtime must read ROM shadow through its platform mapping and restrict shadow
writes and SRAM metadata initialization to writable backing memory. This mapping
is independent of LLVM's stack-shadow offset when access checks are outlined.

The compiler byte generation and associated-section approach are adapted from
[`schrodingerzy/asan-replay`](https://github.com/SchrodingerZhu/llvm-project/tree/f634e199ba6c7ef52b72a15268233a1477080185).
Its split-address mapping and automatic ROM/RAM shadow routing are not required
by this output-section mapping.
