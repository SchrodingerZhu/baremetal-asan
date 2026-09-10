use std::{env, path::PathBuf};

fn main() {
    let libc = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../libc");
    for directory in ["src/__support", "src/string/memory_utils", "hdr", "include"] {
        println!("cargo:rerun-if-changed={}", libc.join(directory).display());
    }
    println!("cargo:rerun-if-changed=src/heap/allocator.cpp");
    println!("cargo:rerun-if-changed=src/heap/allocator.h");

    // Use our arena-based FFI, without libc's global heap or malloc entrypoints.
    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .cpp_link_stdlib(None)
        .include(&libc)
        .file(libc.join("src/__support/freelist.cpp"))
        .file(libc.join("src/__support/freetrie.cpp"))
        .file("src/heap/allocator.cpp")
        .define("LIBC_NAMESPACE", "__llvm_libc_asan")
        .define("LIBC_FULL_BUILD", None)
        .define("LIBC_COPT_BAREMETAL_HEAP_ENABLE_FREESTORE_ROTATION", None)
        // Standalone allocator support has no libc assertion/exit backend.
        .define("NDEBUG", None)
        .flag("-ffreestanding")
        .flag("-fno-exceptions")
        .flag("-fno-rtti")
        .flag("-nostdinc++")
        .flag("-fno-sanitize=all")
        // Bundle native objects so Rust ThinLTO does not depend on the C++
        // compiler using the same LLVM bitcode version as rustc.
        .flag("-fno-lto")
        .compile("llvm_libc_allocator");
}
