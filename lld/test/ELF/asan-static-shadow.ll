; REQUIRES: arm
; RUN: split-file %s %t
; RUN: opt %t/globals.ll -passes=asan -asan-create-global-shadow -asan-globals-metadata-section=asan_globals -asan-constructor-kind=none -o %t/g8.bc
; RUN: llc %t/g8.bc -data-sections -filetype=obj -o %t/g8.o
; RUN: ld.lld %t/g8.o -T %t/layout.ld --asan-shadow-section=.globals:.shadow -o %t/g8
; RUN: llvm-readelf -x .shadow -s %t/g8 | FileCheck %s --check-prefixes=SYMS,G8
; RUN: opt %t/globals.ll -passes=asan -asan-create-global-shadow -asan-globals-metadata-section=asan_globals -asan-constructor-kind=none -asan-mapping-scale=4 -o %t/g16.bc
; RUN: llc %t/g16.bc -data-sections -filetype=obj -o %t/g16.o
; RUN: ld.lld %t/g16.o -T %t/layout.ld --asan-shadow-section=.globals:.shadow --asan-shadow-scale=4 -o %t/g16
; RUN: llvm-readelf -x .shadow -s %t/g16 | FileCheck %s --check-prefixes=SYMS,G16

;; Partial links retain association; placement is performed only at final link.
; RUN: ld.lld -r %t/g8.o --asan-shadow-section=.globals:.shadow -o %t/partial.o
; RUN: ld.lld %t/partial.o -T %t/layout.ld --asan-shadow-section=.globals:.shadow -o %t/partial
; RUN: llvm-readelf -x .shadow %t/partial | FileCheck %s --check-prefix=G8

;; A full LTO link must retain both the precomputed bytes and their association.
; RUN: ld.lld %t/g8.bc -T %t/layout.ld --asan-shadow-section=.globals:.shadow -o %t/lto
; RUN: llvm-readelf -x .shadow -s %t/lto | FileCheck %s --check-prefixes=SYMS,G8
; RUN: opt %t/g8.bc -passes=name-anon-globals -module-summary -o %t/thin.bc
; RUN: ld.lld %t/thin.bc -T %t/layout.ld --asan-shadow-section=.globals:.shadow -o %t/thin
; RUN: llvm-readelf -x .shadow -s %t/thin | FileCheck %s --check-prefixes=SYMS,G8

;; The linker detects mismatched scales and non-unique global input sections.
; RUN: not ld.lld %t/g8.o -T %t/layout.ld --asan-shadow-section=.globals:.shadow --asan-shadow-scale=4 -o /dev/null 2>&1 | FileCheck %s --check-prefix=BAD-SIZE
; RUN: llc %t/g8.bc -filetype=obj -o %t/grouped.o
; RUN: not ld.lld %t/grouped.o -T %t/grouped.ld --asan-shadow-section=.globals:.shadow -o /dev/null 2>&1 | FileCheck %s --check-prefix=BAD-SIZE

; SYMS-DAG: 10000000 {{.*}} a
; SYMS-DAG: 10000080 {{.*}} b
; SYMS-DAG: 22000000 {{.*}} rw
; G8: Hex dump of section '.shadow':
; G8-NEXT: 0x18000000 000001f9 f9f9f9f9 00000000 00000000
; G8-NEXT: 0x18000010 00f9f9f9 00000000
; G16: Hex dump of section '.shadow':
; G16-NEXT: 0x18000000 0001f9f9 00000000 08f90000
; BAD-SIZE: static ASan shadow size/alignment does not match its global

;--- globals.ll
target triple = "thumbv8m.main-none-eabi"
@b = constant [8 x i8] zeroinitializer, align 1
@a = constant [17 x i8] zeroinitializer, align 1
@rw = global i32 42, align 4

;--- layout.ld
ENTRY(a)
SECTIONS {
  .globals 0x10000000 : {
    *(.rodata.a)
    . = ALIGN(128);
    *(.rodata.b)
    . += 32;
  }
  .shadow 0x18000000 : {
    *(__shadow_ro)
    __shadow_end = .;
  }
  ASSERT(__shadow_end == ADDR(.shadow) + SIZEOF(.shadow), "shadow end")
  .metadata : { *(asan_globals) *(.rodata*) }
  .data 0x22000000 : { *(.data*) }
  /DISCARD/ : { *(.ARM.exidx*) }
}

;--- grouped.ld
ENTRY(a)
SECTIONS {
  .globals 0x10000000 : { *(.rodata*) }
  .shadow 0x18000000 : { *(__shadow_ro) }
  .metadata : { *(asan_globals) }
  .data 0x22000000 : { *(.data*) }
  /DISCARD/ : { *(.ARM.exidx*) }
}
