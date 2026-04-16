//! Alloc-based [`JITMemoryProvider`] for `no_std` JIT (required by `cranelift-jit` without `system-memory`).

use alloc::alloc::{Layout, alloc, dealloc};
use alloc::vec::Vec;

use cranelift_jit::{BranchProtection, JITMemoryProvider, io};
use cranelift_module::ModuleResult;

/// Heap-backed JIT memory without OS protection changes (embedded-friendly).
pub(crate) struct AllocJitMemoryProvider {
    allocations: Vec<(Layout, *mut u8)>,
}

impl AllocJitMemoryProvider {
    pub(crate) fn new() -> Self {
        Self {
            allocations: Vec::new(),
        }
    }
}

unsafe impl Send for AllocJitMemoryProvider {}

impl JITMemoryProvider for AllocJitMemoryProvider {
    fn allocate_readexec(&mut self, size: usize, align: u64) -> io::Result<*mut u8> {
        let align = usize::try_from(align).map_err(|_| io::Error)?;
        let layout = Layout::from_size_align(size, align).map_err(|_| io::Error)?;

        unsafe {
            let ptr = alloc(layout);
            if ptr.is_null() {
                return Err(io::Error);
            }
            self.allocations.push((layout, ptr));
            Ok(ptr)
        }
    }

    fn allocate_readwrite(&mut self, size: usize, align: u64) -> io::Result<*mut u8> {
        self.allocate_readexec(size, align)
    }

    fn allocate_readonly(&mut self, size: usize, align: u64) -> io::Result<*mut u8> {
        self.allocate_readexec(size, align)
    }

    unsafe fn free_memory(&mut self) {
        for (layout, ptr) in self.allocations.drain(..) {
            unsafe {
                dealloc(ptr, layout);
            }
        }
    }

    fn finalize(&mut self, _branch_protection: BranchProtection) -> ModuleResult<()> {
        Ok(())
    }
}
