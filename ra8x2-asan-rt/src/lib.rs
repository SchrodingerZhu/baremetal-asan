#![no_std]
//! RA8x2 static ASan runtime, using the selected RA8M2 memory platform.

pub mod platform;

baremetal_asan_rt::export_asan!(platform::ActivePlatform);

#[cfg(all(target_arch = "arm", target_os = "none"))]
use semihosting as _;
