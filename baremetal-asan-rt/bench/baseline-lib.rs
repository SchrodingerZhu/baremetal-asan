#![no_std]
mod baseline;
mod diagnostic {
    pub(crate) fn report_access(addr: usize, size: usize, is_write: bool, invalid: usize) -> ! {
        unsafe extern "C" {
            fn benchmark_failure(addr: usize, size: usize, is_write: bool, invalid: usize) -> !;
        }
        unsafe { benchmark_failure(addr, size, is_write, invalid) }
    }
}
