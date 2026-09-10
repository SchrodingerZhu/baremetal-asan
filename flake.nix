{
  description = "Patched LLVM tools and bare-metal ASan runtime for RA8xx";

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

      rustPlatform = pkgs.makeRustPlatform {
        cargo = rustToolchain;
        rustc = rustToolchain;
      };

      # Local path inputs can include bytecode left by previous LLVM tests.
      sourceFiles = paths: pkgs.lib.fileset.unions (map
        (path: pkgs.lib.fileset.fileFilter
          (file: !(file.hasExt "pyc" || file.hasExt "pyo")) path)
        paths);

      asanLLVM = llvm.libcxxStdenv.mkDerivation {
        pname = "baremetal-asan-llvm";
        version = "24.0.0-dev";
        src = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = sourceFiles [
            ./llvm ./clang ./lld ./cmake ./third-party ./libc ./libunwind
          ];
        };
        cmakeDir = "../llvm";
        nativeBuildInputs = [ pkgs.cmake pkgs.ninja pkgs.python3 pkgs.perl ];
        buildInputs = [ pkgs.zlib pkgs.zstd ];
        cmakeFlags = [
          "-DCMAKE_BUILD_TYPE=Release"
          "-DLLVM_ENABLE_PROJECTS=clang;lld"
          "-DLLVM_TARGETS_TO_BUILD=Native;ARM"
          "-DLLVM_ENABLE_LIBCXX=ON"
          "-DLLVM_USE_LINKER=${llvm.bintools}/bin/ld.lld"
          "-DLLVM_ENABLE_ASSERTIONS=ON"
          "-DLLVM_APPEND_VC_REV=OFF"
          "-DLLVM_INCLUDE_TESTS=OFF"
          "-DLLVM_INCLUDE_BENCHMARKS=OFF"
          "-DLLVM_INCLUDE_EXAMPLES=OFF"
          "-DLLVM_ENABLE_BINDINGS=OFF"
          "-DLLVM_ENABLE_LIBEDIT=OFF"
          "-DLLVM_ENABLE_LIBXML2=OFF"
          "-DLLVM_DISTRIBUTION_COMPONENTS=clang;clang-resource-headers;lld;llvm-ar;llvm-ranlib;llvm-nm;llvm-objdump;llvm-objcopy;llvm-readobj;llvm-readelf;llvm-size"
        ];
        enableParallelBuilding = true;
        ninjaFlags = [ "distribution" ];
        installTargets = [ "install-distribution" ];
      };

      asanRuntime = pkgs.stdenvNoCC.mkDerivation {
        pname = "ra8x2-asan-rt";
        version = "0.1.0";
        src = pkgs.lib.fileset.toSource {
          root = ./.;
          fileset = sourceFiles [
            ./Cargo.toml ./Cargo.lock ./baremetal-asan-rt ./ra8x2-asan-rt ./libc
          ];
        };
        cargoDeps = rustPlatform.importCargoLock { lockFile = ./Cargo.lock; };
        nativeBuildInputs = [
          rustToolchain rustPlatform.cargoSetupHook llvm.clang-unwrapped llvm.llvm
        ];
        CXX = "${llvm.clang-unwrapped}/bin/clang++";
        AR = "${llvm.llvm}/bin/llvm-ar";
        CXX_thumbv8m_main_none_eabihf = "${llvm.clang-unwrapped}/bin/clang++";
        AR_thumbv8m_main_none_eabihf = "${llvm.llvm}/bin/llvm-ar";
        dontConfigure = true;
        dontStrip = true;
        buildPhase = ''
          runHook preBuild
          cargo rustc --offline --locked -p ra8x2-asan-rt --release \
            --no-default-features --target thumbv8m.main-none-eabihf \
            -- -C target-cpu=cortex-m85
          runHook postBuild
        '';
        installPhase = ''
          runHook preInstall
          mkdir -p "$out/lib"
          cp target/thumbv8m.main-none-eabihf/release/libra8x2_asan_rt.a "$out/lib/"
          runHook postInstall
        '';
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
      packages.${system} = {
        llvm = asanLLVM;
        ra8x2-asan-rt = asanRuntime;
        default = asanLLVM;
      };

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
          export PATH=${rustToolchain}/bin:${llvm.libcxxStdenv.cc}/bin:${llvm.bintools}/bin:${llvm.llvm}/bin:$PATH
          export CC=${llvm.libcxxStdenv.cc}/bin/clang
          export CXX=${llvm.libcxxStdenv.cc}/bin/clang++
          export AR=${llvm.llvm}/bin/llvm-ar
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
