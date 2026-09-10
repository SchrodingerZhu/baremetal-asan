# REQUIRES: x86
# RUN: split-file %s %t
# RUN: llvm-mc -filetype=obj -triple=x86_64 %t/a.s -o %t/a.o
# RUN: llvm-mc -filetype=obj -triple=x86_64 %t/b.s -o %t/b.o
# RUN: ld.lld %t/b.o %t/a.o -T %t/layout.ld --asan-shadow-section=.globals:.shadow -o %t/out
# RUN: llvm-readelf -x .shadow %t/out | FileCheck %s --check-prefix=BYTES

## GC removes a shadow with its target; an all-dead pair needs no storage.
# RUN: ld.lld %t/b.o %t/a.o -T %t/layout.ld --asan-shadow-section=.globals:.shadow --gc-sections -o %t/gc
# RUN: llvm-readelf -x .shadow %t/gc | FileCheck %s --check-prefix=GC
# RUN: ld.lld %t/b.o %t/a.o -e 0 -T %t/layout.ld --asan-shadow-section=.globals:.shadow --gc-sections -o %t/dead
# RUN: llvm-readelf -S %t/dead | FileCheck %s --check-prefix=DEAD --implicit-check-not=.globals --implicit-check-not=.shadow

## Forward references: the shadow output appears before its globals in the script.
# RUN: ld.lld %t/b.o %t/a.o -T %t/forward.ld --asan-shadow-section=.globals:.shadow -o %t/forward
# RUN: llvm-readelf -x .shadow %t/forward | FileCheck %s --check-prefix=BYTES
# RUN: ld.lld %t/b.o %t/a.o -T %t/prefix.ld --asan-shadow-section=.globals:.shadow -o %t/prefix
# RUN: llvm-readelf -x .shadow %t/prefix | FileCheck %s --check-prefix=PREFIX
# RUN: ld.lld %t/b.o %t/a.o -T %t/memory.ld --asan-shadow-section=.globals:.shadow -o %t/memory
# RUN: llvm-readelf -x .shadow %t/memory | FileCheck %s --check-prefix=BYTES

## Preserve input-description boundaries and intervening symbol assignments.
# RUN: ld.lld %t/b.o %t/a.o -T %t/groups.ld --asan-shadow-section=.globals:.shadow -o %t/groups
# RUN: llvm-readelf -x .shadow %t/groups | FileCheck %s --check-prefix=BYTES

## Do not silently accept scripts or alignments that prevent correct placement.
# RUN: not ld.lld %t/b.o %t/a.o -T %t/bad-order.ld --asan-shadow-section=.globals:.shadow -o /dev/null 2>&1 | FileCheck %s --check-prefix=ORDER
# RUN: not ld.lld %t/b.o %t/a.o -T %t/bad-align.ld --asan-shadow-section=.globals:.shadow -o /dev/null 2>&1 | FileCheck %s --check-prefix=ORDER
# RUN: not ld.lld %t/a.o -T %t/layout.ld --asan-shadow-section=.missing:.also_missing -o /dev/null 2>&1 | FileCheck %s --check-prefix=MISSING
# RUN: not ld.lld %t/a.o --asan-shadow-section=.same:.same -o /dev/null 2>&1 | FileCheck %s --check-prefix=SYNTAX
# RUN: not ld.lld %t/a.o --asan-shadow-section=.globals:.shadow --asan-shadow-scale=8 -o /dev/null 2>&1 | FileCheck %s --check-prefix=SCALE
# RUN: not ld.lld %t/a.o --asan-shadow-section=.globals:.shadow --asan-shadow-scale=4294967299 -o /dev/null 2>&1 | FileCheck %s --check-prefix=SCALE
# RUN: not ld.lld %t/a.o --asan-shadow-section=.globals:.shadow --asan-shadow-scale=-1 -o /dev/null 2>&1 | FileCheck %s --check-prefix=SCALE
# RUN: not ld.lld %t/a.o --asan-shadow-scale=3 -o /dev/null 2>&1 | FileCheck %s --check-prefix=PAIR
# RUN: not ld.lld %t/a.o --asan-shadow-section=.globals:.shadow -pie -o /dev/null 2>&1 | FileCheck %s --check-prefix=STATIC

# BYTES: Hex dump of section '.shadow':
# BYTES-NEXT: 0x18000000 00f9f9f9 00000000 00000000 00000000
# BYTES-NEXT: 0x18000010 01f9f9f9
# PREFIX: Hex dump of section '.shadow':
# PREFIX-NEXT: 0x18000000 00000000 00f9f9f9 00000000 00000000
# PREFIX-NEXT: 0x18000010 01f9f9f9
# GC: Hex dump of section '.shadow':
# GC-NEXT: 0x18000000 00f9f9f9
# GC-EMPTY:
# DEAD: Section Headers:
# ORDER: cannot place static ASan shadow at offset
# MISSING: --asan-shadow-section requires output sections .missing and .also_missing
# SYNTAX: --asan-shadow-section expects distinct output sections <globals>:<shadow>
# SCALE: --asan-shadow-scale must be between 0 and 7
# PAIR: --asan-shadow-scale requires --asan-shadow-section
# STATIC: --asan-shadow-section requires a static executable

#--- a.s
.section .rodata.a,"a",@progbits
.p2align 5
.globl a
a:
.zero 32
.section __shadow_ro,"ao",@progbits,a
.byte 0, 0xf9, 0xf9, 0xf9

#--- b.s
.section .rodata.b,"a",@progbits
.p2align 7
.globl b
b:
.zero 32
.section __shadow_ro,"ao",@progbits,b
.byte 1, 0xf9, 0xf9, 0xf9

#--- layout.ld
ENTRY(a)
SECTIONS {
  .globals 0x10000000 : { *(SORT_BY_NAME(.rodata.*)) }
  .shadow 0x18000000 : { *(__shadow_ro) }
}

#--- forward.ld
ENTRY(a)
PHDRS { shadow PT_LOAD; globals PT_LOAD; }
SECTIONS {
  .shadow 0x18000000 : { *(__shadow_ro) } :shadow
  .globals 0x10000000 : { *(SORT_BY_NAME(.rodata.*)) } :globals
}

#--- prefix.ld
ENTRY(a)
SECTIONS {
  .globals 0x10000000 : { . += 32; *(SORT_BY_NAME(.rodata.*)) }
  .shadow 0x18000000 : { *(__shadow_ro) }
}

#--- groups.ld
ENTRY(a)
SECTIONS {
  .globals 0x10000000 : { *(SORT_BY_NAME(.rodata.*)) }
  .shadow 0x18000000 : {
    *a.o(__shadow_ro)
    after_a = .;
    *b.o(__shadow_ro)
    after_b = .;
  }
  ASSERT(after_a == 0x18000004, "after a")
  ASSERT(after_b == 0x18000014, "after b")
}

#--- memory.ld
ENTRY(a)
MEMORY {
  GLOBALS (r) : ORIGIN = 0x10000000, LENGTH = 160
  SHADOW (r) : ORIGIN = 0x18000000, LENGTH = 20
}
SECTIONS {
  .globals : { *(SORT_BY_NAME(.rodata.*)) } > GLOBALS
  .shadow : { *(__shadow_ro) } > SHADOW
}

#--- bad-order.ld
ENTRY(a)
SECTIONS {
  .globals 0x10000000 : { *(SORT_BY_NAME(.rodata.*)) }
  .shadow 0x18000000 : { *b.o(__shadow_ro) *a.o(__shadow_ro) }
}

#--- bad-align.ld
ENTRY(a)
SECTIONS {
  .globals 0x10000000 : { *(.rodata.a) *(.rodata.b) }
  .shadow 0x18000000 : SUBALIGN(32) { *(__shadow_ro) }
}
