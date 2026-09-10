; RUN: opt %s -passes=asan -asan-create-global-shadow -asan-globals-metadata-section=asan_globals -S | FileCheck %s --check-prefixes=COMMON,G8 --implicit-check-not=__asan_global_shadow_rw --implicit-check-not=__asan_global_shadow_dyn
; RUN: opt %s -passes=asan -asan-create-global-shadow -asan-globals-metadata-section=asan_globals -asan-mapping-scale=4 -S | FileCheck %s --check-prefixes=COMMON,G16
; RUN: opt %s -passes='asan,constmerge,globaldce' -asan-create-global-shadow -asan-globals-metadata-section=asan_globals -S | FileCheck %s --check-prefixes=COMMON,G8
; RUN: opt %s -passes=asan -asan-globals-metadata-section=asan_globals -S | FileCheck %s --check-prefix=OFF --implicit-check-not=__asan_global_shadow_
; RUN: not opt %s -passes=asan -asan-create-global-shadow -disable-output 2>&1 | FileCheck %s --check-prefix=ERROR
; RUN: not opt %s -mtriple=x86_64-apple-darwin -passes=asan -asan-create-global-shadow -asan-globals-metadata-section=asan_globals -disable-output 2>&1 | FileCheck %s --check-prefix=ERROR

target triple = "x86_64-unknown-linux-gnu"

@partial = constant [17 x i8] zeroinitializer, align 1
@aligned = constant [16 x i8] zeroinitializer, align 1
@small = constant [1 x i8] zeroinitializer, align 1
@empty = constant [0 x i8] zeroinitializer, align 1
@rw = global i32 1, align 4
@dyn = constant i32 0, align 4, sanitize_address_dyninit

; G8-DAG: @__asan_global_shadow_partial = private constant [8 x i8] c"\00\00\01\F9\F9\F9\F9\F9", section "__shadow_ro", align 1, !associated ![[PARTIAL:[0-9]+]]
; G8-DAG: @__asan_global_shadow_aligned = private constant [4 x i8] c"\00\00\F9\F9", section "__shadow_ro", align 1
; G8-DAG: @__asan_global_shadow_small = private constant [4 x i8] c"\01\F9\F9\F9", section "__shadow_ro", align 1
; G8-DAG: @__asan_global_shadow_empty = private constant [4 x i8] c"\F9\F9\F9\F9", section "__shadow_ro", align 1
; G16-DAG: @__asan_global_shadow_partial = private constant [4 x i8] c"\00\01\F9\F9", section "__shadow_ro", align 1, !associated ![[PARTIAL:[0-9]+]]
; G16-DAG: @__asan_global_shadow_aligned = private constant [2 x i8] c"\00\F9", section "__shadow_ro", align 1
; G16-DAG: @__asan_global_shadow_small = private constant [2 x i8] c"\01\F9", section "__shadow_ro", align 1
; G16-DAG: @__asan_global_shadow_empty = private constant [2 x i8] c"\F9\F9", section "__shadow_ro", align 1
; COMMON-DAG: @__asan_global_rw = {{.*}} section "asan_globals"
; COMMON-DAG: @__asan_global_partial = {{.*}} section "asan_globals"
; COMMON-DAG: @llvm.compiler.used = {{.*}}ptr @__asan_global_shadow_partial
; COMMON: define internal void @asan.module_ctor()
; COMMON-NOT: call void @__asan_register
; COMMON: ret void
; COMMON: ![[PARTIAL]] = !{ptr @partial}
; OFF: @__asan_global_rw = {{.*}} section "asan_globals"
; ERROR: error: -asan-create-global-shadow requires an ELF target and -asan-globals-metadata-section
