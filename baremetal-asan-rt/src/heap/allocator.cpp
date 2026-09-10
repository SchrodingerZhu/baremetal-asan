#include "allocator.h"
#include "src/__support/freelist_heap.h"

using namespace LIBC_NAMESPACE;

extern "C" BaremetalAsanHeap *__baremetal_asan_heap_init(void *base,
                                                         size_t size) {
  uintptr_t start = reinterpret_cast<uintptr_t>(base);
  if (!base || size > PTRDIFF_MAX || start > UINTPTR_MAX - size)
    return nullptr;

  size_t padding = -start & (alignof(FreeListHeap) - 1);
  if (padding > size || sizeof(FreeListHeap) > size - padding)
    return nullptr;

  auto *storage = static_cast<cpp::byte *>(base) + padding;
  ByteSpan pool(storage + sizeof(FreeListHeap),
                size - padding - sizeof(FreeListHeap));
  // Validate the remaining pool using libc's own block layout. FreeListHeap
  // initializes its free index lazily on the first allocation.
  auto first = BlockRef::init(pool);
  if (!first || FreeStore::too_small(*first))
    return nullptr;

  auto *heap = new (storage) FreeListHeap(pool);
  return reinterpret_cast<BaremetalAsanHeap *>(heap);
}

extern "C" void *__baremetal_asan_heap_allocate(BaremetalAsanHeap *heap,
                                                size_t size, size_t alignment) {
  if (!heap || !size || !IsPow2(alignment))
    return nullptr;

  size_t rounded;
  if (add_overflow(size, alignment - 1, rounded) || rounded > PTRDIFF_MAX)
    return nullptr;
  rounded &= ~(alignment - 1);
  // libc's aligned_allocate requires size to be a multiple of alignment.
  return reinterpret_cast<FreeListHeap *>(heap)->aligned_allocate(alignment,
                                                                  rounded);
}

extern "C" void __baremetal_asan_heap_deallocate(BaremetalAsanHeap *heap,
                                                 void *ptr) {
  if (heap && ptr)
    reinterpret_cast<FreeListHeap *>(heap)->free(ptr);
}
