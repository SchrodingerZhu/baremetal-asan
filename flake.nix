{
  description = "LLVM 23/libc++ development environment for bare-metal ASan on RA8xx";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };
      llvm = pkgs.llvmPackages_23;

      rustToolchain = pkgs.rust-bin.stable.latest.default.override {
        targets = [ "thumbv8m.main-none-eabihf" ];
        extensions = [ "rust-src" ];
      };

      python = pkgs.python3.withPackages (ps: with ps; [
        pyyaml psutil
      ]);

      configureLLVM = pkgs.writeShellScriptBin "configure-llvm" ''
        set -eu
        exec cmake -S llvm -B build/llvm -G Ninja \
          -DCMAKE_BUILD_TYPE=Release \
          -DCMAKE_C_COMPILER=${llvm.libcxxStdenv.cc}/bin/clang \
          -DCMAKE_CXX_COMPILER=${llvm.libcxxStdenv.cc}/bin/clang++ \
          -DCMAKE_C_COMPILER_LAUNCHER= \
          -DCMAKE_CXX_COMPILER_LAUNCHER= \
          -DCMAKE_EXPORT_COMPILE_COMMANDS=ON \
          '-DLLVM_ENABLE_PROJECTS=clang;lld' \
          '-DLLVM_TARGETS_TO_BUILD=Native;ARM' \
          -DLLVM_ENABLE_LIBCXX=ON \
          -DLLVM_USE_LINKER=${llvm.bintools}/bin/ld.lld \
          -DLLVM_ENABLE_ASSERTIONS=ON \
          -DLLVM_CCACHE_BUILD=OFF \
          -DLLVM_INCLUDE_BENCHMARKS=OFF \
          "$@"
      '';
    in {
      devShells.${system}.default = (pkgs.mkShell.override {
        stdenv = llvm.libcxxStdenv;
      }) {
        name = "baremetal-asan";
        packages = [
          llvm.bintools
          llvm.llvm
          configureLLVM
          pkgs.probe-rs-tools
          pkgs.cmake
          pkgs.ninja
          pkgs.git
          pkgs.perl
          rustToolchain
          pkgs.pkg-config
          python
        ];
        buildInputs = with pkgs; [ zlib zstd libxml2 libedit systemd ];

        shellHook = ''
          unset CPLUS_INCLUDE_PATH C_INCLUDE_PATH CPATH
          unset RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
          export PATH=${llvm.libcxxStdenv.cc}/bin:${llvm.bintools}/bin:${llvm.llvm}/bin:$PATH
          export CC=${llvm.libcxxStdenv.cc}/bin/clang
          export CXX=${llvm.libcxxStdenv.cc}/bin/clang++
          export CMAKE_C_COMPILER_LAUNCHER=
          export CMAKE_CXX_COMPILER_LAUNCHER=
          export RA8XX_COMPILER_LAUNCHER=
          export RA8XX_TRIPLE=armv8.1m.main-none-eabi
          export RA8XX_CPU="''${RA8XX_CPU:-cortex-m85}"
          export RA8XX_CFLAGS="-mcpu=$RA8XX_CPU -mfloat-abi=hard -mthumb"
          export RA8XX_SERIAL="''${RA8XX_SERIAL:-/dev/ttyACM0}"
          export RA8XX_BAUD="''${RA8XX_BAUD:-921600}"
          unset shellHook buildPhase
        '';
      };
    };
}
