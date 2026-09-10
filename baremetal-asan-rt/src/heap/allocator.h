#ifndef BAREMETAL_ASAN_HEAP_ALLOCATOR_H
#define BAREMETAL_ASAN_HEAP_ALLOCATOR_H

#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

struct BaremetalAsanHeap;

// Construct a heap inside the writable arena, reserving its prefix for
// allocator state. Returns NULL if the arena is absent, wraps, or is too small.
// The arena must remain exclusively owned by this heap and its allocations.
// Initialize it only once, and serialize all operations on the returned handle.
struct BaremetalAsanHeap *__baremetal_asan_heap_init(void *base, size_t size);

// Allocate uninitialized bytes, with power-of-two alignment. The size need not
// be an alignment multiple. Returns NULL for an invalid/zero request or OOM.
void *__baremetal_asan_heap_allocate(struct BaremetalAsanHeap *heap,
                                     size_t size, size_t alignment);

// Quarantine a live allocation from this heap. NULL is a no-op. All accesses to
// the allocation must finish before this call; double/foreign frees are
// invalid.
void __baremetal_asan_heap_deallocate(struct BaremetalAsanHeap *heap,
                                      void *ptr);

#ifdef __cplusplus
}
#endif

#endif
