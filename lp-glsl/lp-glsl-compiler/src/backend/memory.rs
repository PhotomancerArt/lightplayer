//! JIT memory provider for no_std environments.
//!
//! Provides a simple alloc-based memory provider that uses standard heap allocation.
//! No special memory protections are applied (suitable for embedded systems like ESP32).

use alloc::alloc::{Layout, alloc, dealloc};
use alloc::vec::Vec;
use cranelift_jit::{BranchProtection, JITMemoryProvider, io};
use cranelift_module::ModuleResult;

/// Simple alloc-based memory provider for no_std environments.
///
/// Uses standard heap allocation without special memory protections.
/// Suitable for embedded systems where memory protection is not available or needed.
pub struct AllocJitMemoryProvider {
    allocations: Vec<(Layout, *mut u8)>,
}

impl AllocJitMemoryProvider {
    /// Create a new alloc-based memory provider.
    pub fn new() -> Self {
        Self {
            allocations: Vec::new(),
        }
    }
}

impl Default for AllocJitMemoryProvider {
    fn default() -> Self {
        Self::new()
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
        // Same as readexec - no special protections on embedded systems
        self.allocate_readexec(size, align)
    }

    fn allocate_readonly(&mut self, size: usize, align: u64) -> io::Result<*mut u8> {
        // Same as readexec - no special protections on embedded systems
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
        // No-op: embedded systems don't need memory protection changes
        // Memory is already allocated and ready to use
        Ok(())
    }
}
