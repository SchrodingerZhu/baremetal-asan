#![no_std]
//! RA8x2 static ASan runtime, using the selected RA8M2 memory layout.

pub mod layout;
mod platform;

baremetal_asan_rt::export_asan!(layout::ActiveLayout, stack_top = platform::stack_top);

#[cfg(all(target_arch = "arm", target_os = "none"))]
use semihosting as _;
