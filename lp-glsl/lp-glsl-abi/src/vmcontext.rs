//! Fixed-layout header at the start of every VMContext allocation.
//!
//! On the reference embedded target (32-bit pointer), [`VmContext`] is 16 bytes. On 64-bit hosts
//! the `metadata` pointer is wider and the struct is larger; use [`core::mem::offset_of!`] (or the
//! `VMCTX_OFFSET_*` constants) instead of assuming a single cross-target size.

use alloc::boxed::Box;

use crate::GlslType;
use crate::GlslValue;

/// Default instruction fuel for new [`VmContext`] values (tests and host JIT calls).
pub const DEFAULT_VMCTX_FUEL: u64 = 1_000_000;

/// Byte offset of [`VmContext::fuel`].
pub const VMCTX_OFFSET_FUEL: usize = core::mem::offset_of!(VmContext, fuel);
/// Byte offset of [`VmContext::trap_handler`].
pub const VMCTX_OFFSET_TRAP_HANDLER: usize = core::mem::offset_of!(VmContext, trap_handler);
/// Byte offset of [`VmContext::metadata`].
pub const VMCTX_OFFSET_METADATA: usize = core::mem::offset_of!(VmContext, metadata);
/// Size of [`VmContext`] in bytes (target-dependent).
pub const VMCTX_HEADER_SIZE: usize = core::mem::size_of::<VmContext>();

/// Well-known fields at the start of every VMContext (single flat allocation).
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VmContext {
    pub fuel: u64,
    pub trap_handler: u32,
    /// Describes globals/uniforms layout; may be null until wired up.
    pub metadata: *const GlslType,
}

// SAFETY: `metadata` is an opaque pointer in the ABI header; hosts that share a `VmContext` across
// threads must synchronize updates. The type is plain data suitable for `repr(C)` layouts.
unsafe impl Send for VmContext {}
unsafe impl Sync for VmContext {}

impl Default for VmContext {
    fn default() -> Self {
        Self {
            fuel: DEFAULT_VMCTX_FUEL,
            trap_handler: 0,
            metadata: core::ptr::null(),
        }
    }
}

/// Historical name used by some call sites; identical to [`VmContext`].
pub type VmContextHeader = VmContext;

impl VmContext {
    /// Size of this header before any globals/uniforms storage.
    pub const HEADER_SIZE: usize = core::mem::size_of::<VmContext>();

    /// Read-only base pointer to storage immediately following this header.
    pub fn globals_base(&self) -> *const u8 {
        (self as *const Self as *const u8).wrapping_add(Self::HEADER_SIZE)
    }

    /// Mutable base pointer to storage immediately following this header.
    pub fn globals_base_mut(&mut self) -> *mut u8 {
        (self as *mut Self as *mut u8).wrapping_add(Self::HEADER_SIZE)
    }

    /// Placeholder: read global by index.
    pub fn get_global(&self, _index: usize) -> GlslValue {
        unimplemented!("globals access in Milestone 2")
    }

    /// Placeholder: write global by index.
    pub fn set_global(&mut self, _index: usize, _value: GlslValue) {
        unimplemented!("globals access in Milestone 2")
    }

    /// Placeholder: read uniform by index.
    pub fn get_uniform(&self, _index: usize) -> GlslValue {
        unimplemented!("uniforms access in Milestone 2")
    }
}

/// Allocate a header-sized buffer initialized from [`VmContext::default`].
pub fn minimal_vmcontext() -> Box<[u8]> {
    let header = VmContext::default();
    let bytes = unsafe {
        core::slice::from_raw_parts(
            &header as *const VmContext as *const u8,
            core::mem::size_of::<VmContext>(),
        )
    };
    bytes.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_offsets_match_constants() {
        let header = VmContext::default();
        let base = core::ptr::addr_of!(header) as usize;
        assert_eq!(
            core::ptr::addr_of!(header.fuel) as usize - base,
            VMCTX_OFFSET_FUEL
        );
        assert_eq!(
            core::ptr::addr_of!(header.trap_handler) as usize - base,
            VMCTX_OFFSET_TRAP_HANDLER
        );
        assert_eq!(
            core::ptr::addr_of!(header.metadata) as usize - base,
            VMCTX_OFFSET_METADATA
        );
    }

    #[test]
    fn minimal_vmcontext_len() {
        let b = minimal_vmcontext();
        assert_eq!(b.len(), VMCTX_HEADER_SIZE);
    }
}
