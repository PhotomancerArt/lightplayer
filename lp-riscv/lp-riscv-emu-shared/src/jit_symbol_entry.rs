//! ABI struct shared between guest (`lp-perf::sinks::syscall`) and host
//! (`lp-riscv-emu` syscall handler) for `SYSCALL_JIT_MAP_LOAD`.
//!
//! Each entry describes one JIT-emitted function: its byte offset within
//! the module's code buffer, its size in bytes, and a guest pointer to a
//! UTF-8 name string.

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct JitSymbolEntry {
    /// Byte offset within the JIT module's code buffer.
    pub offset: u32,
    /// Function size in bytes (derived from sorted-offset deltas at emit time).
    pub size: u32,
    /// Guest pointer to the UTF-8 name string.
    pub name_ptr: u32,
    /// Length of the name in bytes.
    pub name_len: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn layout_is_four_u32s() {
        assert_eq!(size_of::<JitSymbolEntry>(), 16);
        assert_eq!(align_of::<JitSymbolEntry>(), 4);
    }
}
