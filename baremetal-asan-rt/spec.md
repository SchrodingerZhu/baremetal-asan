# Outlined ASan runtime functions

For Clang/LLVM on bare-metal Arm ELF. All groups assume `-fsanitize=address`; flags below control compiler instrumentation. Braces denote separate names; `0..10` includes every integer from 0 through 10. Descriptions give the intended runtime behavior, including for currently stubbed functions.

## Core — `-fsanitize=address`

| Function(s) | Functionality |
| --- | --- |
| `__asan_init` | Initialize shadow memory and runtime state; tolerate repeated calls. |
| `__asan_version_mismatch_check_v8` | ABI compatibility marker, emitted with `-mllvm -asan-guard-against-version-mismatch=1` (default). |
| `__asan_memcpy` | Check source and destination ranges, copy non-overlapping memory, and return the destination. |
| `__asan_memmove` | Check source and destination ranges, copy with overlap support, and return the destination. |
| `__asan_memset` | Check the destination range, fill it with a byte value, and return the destination. |
| `__asan_handle_no_return` | Clear stack poisoning before a non-returning call; may be emitted even with stack instrumentation disabled. |

## Explicit heap allocation

| Function(s) | Functionality |
| --- | --- |
| `__asan_malloc` | Allocate with poisoned redzones and exact payload shadow using the rotational heap; return null on failure. |
| `__asan_free` | Poison a live allocation and return it to the heap's existing quarantine; accept null. |

## Outlined checks — `-fsanitize-address-outline-instrumentation`

| Function(s) | Functionality |
| --- | --- |
| `__asan_load{1,2,4,8,16}` | Check a read of the indicated byte size; report and terminate if invalid. |
| `__asan_store{1,2,4,8,16}` | Check a write of the indicated byte size; report and terminate if invalid. |
| `__asan_loadN` | Check a read with an explicit byte count, including unusually sized or unaligned accesses. |
| `__asan_storeN` | Check a write with an explicit byte count, including unusually sized or unaligned accesses. |

## Globals — `-mllvm -asan-globals=1` (default)

| Additional flag | Function(s) | Functionality |
| --- | --- | --- |
| `-fsanitize-address-globals-dead-stripping` | `__asan_register_elf_globals`, `__asan_unregister_elf_globals` | Register/unregister ELF section descriptors and poison/unpoison global redzones; prevent duplicate registration using the supplied flag. |
| `-fno-sanitize-address-globals-dead-stripping` | `__asan_register_globals`, `__asan_unregister_globals` | Register/unregister an explicit descriptor array and poison/unpoison global redzones. |
| `-mllvm -asan-initialization-order=1` (default) | `__asan_before_dynamic_init`, `__asan_after_dynamic_init` | Poison globals before dynamic initialization for initialization-order checks, then restore accessibility afterward. |

## Stack — `-mllvm -asan-stack=1` (default)

| Additional flag | Function(s) | Functionality |
| --- | --- | --- |
| `-mllvm -asan-max-inline-poisoning-size=N` (default: 64) | `__asan_set_shadow_{00,01,02,03,04,05,06,07,f1,f2,f3,f5,f8}` | Fill shadow bytes with the hexadecimal suffix value when an update exceeds the inline threshold. |
| `-fsanitize-address-use-after-scope` | `__asan_poison_stack_memory`, `__asan_unpoison_stack_memory` | Mark local-variable ranges inaccessible/accessible at lifetime boundaries, when helper calls are emitted. |
| `-mllvm -asan-instrument-dynamic-allocas=1` (default) | `__asan_alloca_poison`, `__asan_allocas_unpoison` | Set 32-byte dynamic-allocation redzones and partial payload shadow, then clear retired stack granules during cleanup. |
| `-fsanitize-address-use-after-return=runtime` | `__asan_stack_malloc_{0..10}` | Allocate a 64-byte through 64-KiB frame directly from the rotational heap when detection is enabled; return zero when unavailable. |
| `-fsanitize-address-use-after-return=always` | `__asan_stack_malloc_always_{0..10}` | Allocate a fake-stack frame without consulting the runtime enable flag; return zero when unavailable. |
| `-fsanitize-address-use-after-return={runtime,always}` and `-mllvm -asan-max-inline-poisoning-size=0` | `__asan_stack_free_{0..10}` | Poison a returned frame with `0xf5` and release it into the heap's existing quarantine. |

## Inline checks — `-mllvm -asan-instrumentation-with-call-threshold=-1`

These report helpers are needed when linking inline-instrumented code. Without forced outlining, LLVM can also choose inline checks by its default threshold.

| Function(s) | Functionality |
| --- | --- |
| `__asan_report_{load,store}{1,2,4,8,16}` | Report an already-detected invalid access and terminate. |
| `__asan_report_{load,store}_n` | Report an invalid access with an explicit byte count; note the lowercase `_n`. |

## Recovery — `-fsanitize-recover=address`

| Function(s) | Functionality |
| --- | --- |
| `__asan_{load,store}{1,2,4,8,16,N}_noabort` | Outlined access checks that report errors and allow execution to continue. |
| `__asan_report_{load,store}{1,2,4,8,16}_noabort`, `__asan_report_{load,store}_n_noabort` | Report-only equivalents for inline checks that allow execution to continue. |

## Other instrumentation features

| Additional flag | Function(s) | Functionality |
| --- | --- | --- |
| `-fsanitize=pointer-compare` | `__sanitizer_ptr_cmp` | Validate pointer comparisons. |
| `-fsanitize=pointer-subtract` | `__sanitizer_ptr_sub` | Validate pointer subtraction. |
| `-fsanitize-address-field-padding={1,2}` | `__asan_poison_intra_object_redzone`, `__asan_unpoison_intra_object_redzone` | Poison inserted C++ class-padding redzones and clear them during destruction. |
| `-mllvm -asan-force-experiment=<nonzero ID>` | `__asan_exp_{load,store}{1,2,4,8,16,N}` | Outlined access checks carrying an experiment identifier. |

Names and behavior follow the local [LLVM ASan pass](../llvm/lib/Transforms/Instrumentation/AddressSanitizer.cpp), [runtime interface](../compiler-rt/lib/asan/asan_interface_internal.h), and [Clang field-padding instrumentation](../clang/lib/CodeGen/CGClass.cpp).
