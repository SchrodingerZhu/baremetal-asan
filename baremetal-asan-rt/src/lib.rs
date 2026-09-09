#![no_std]

mod abi;
mod diagnostic;
mod global;
mod stack;
mod access;

#[cfg(all(target_arch = "arm", target_os = "none"))]
use semihosting as _;
