//! Fixed-layout header at the start of every VMContext allocation.
//!
//! On the reference embedded target (32-bit pointer), [`VmContext`] is 16 bytes. On 64-bit hosts
//! the `metadata` pointer is wider and the struct is larger; use [`core::mem::offset_of!`] (or the
//! `VMCTX_OFFSET_*` constants) instead of assuming a single cross-target size.
//!
//! # Per-instance vs shared memory
//!
//! [`VmContext`] is **per shader instance** (fuel, trap code, metadata for instance locals).
//! **Shared** data (textures, cross-shader globals) is allocated through
//! [`LpvmEngine::memory`](crate::LpvmEngine::memory) as [`ShaderPtr`](crate::ShaderPtr) values;
//! the guest sees [`ShaderPtr::guest_value`](crate::ShaderPtr::guest_value) via uniforms.

use alloc::boxed::Box;

use lps_shared::LpsType;

/// Default instruction fuel for new [`VmContext`] values (tests and host JIT calls).
pub const DEFAULT_VMCTX_FUEL: u64 = 1_000_000;

/// Default per-invocation (per-pixel / per-sample) fuel budget, in loop
/// back-edge executions. Render wrappers reset the fuel counter to this
/// value at the top of each invocation (calibrated in the fuel plan's P4).
pub const DEFAULT_INVOCATION_FUEL: u32 = 100_000;

/// Value of the invocation-index half of [`VmContext::fuel`] (high u32,
/// offset 4) written by the host when arming the header before a guest
/// entry — i.e. "no per-invocation wrapper has run yet".
pub const INVOCATION_INDEX_ARMED: u32 = 0xFFFF_FFFF;

/// [`VmContext::trap`] code: no trap occurred.
pub const TRAP_CODE_NONE: u32 = 0;
/// [`VmContext::trap`] code: guest observed an exhausted fuel counter and
/// aborted to the function epilogue.
pub const TRAP_CODE_OUT_OF_FUEL: u32 = 1;

/// Byte offset of [`VmContext::fuel`].
pub const VMCTX_OFFSET_FUEL: usize = core::mem::offset_of!(VmContext, fuel);
/// Byte offset of [`VmContext::trap`].
pub const VMCTX_OFFSET_TRAP: usize = core::mem::offset_of!(VmContext, trap);
/// Byte offset of [`VmContext::metadata`].
pub const VMCTX_OFFSET_METADATA: usize = core::mem::offset_of!(VmContext, metadata);
/// Size of [`VmContext`] in bytes (target-dependent).
pub const VMCTX_HEADER_SIZE: usize = core::mem::size_of::<VmContext>();

/// Per-instance VM state at the start of a VMContext allocation.
///
/// Shared heap data is **not** referenced from here; use [`LpvmEngine::memory`](crate::LpvmEngine::memory)
/// and pass [`ShaderPtr::guest_value`](crate::ShaderPtr::guest_value) into shaders as uniforms.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct VmContext {
    /// Fuel word, split into two u32 halves (little-endian layout):
    ///
    /// * **low u32 (offset 0)** — remaining fuel counter. Unit: loop
    ///   back-edge executions. All guest arithmetic is u32; existing
    ///   readers (`__lp_get_fuel`) already truncate to 32 bits.
    /// * **high u32 (offset 4)** — current invocation index (linear
    ///   pixel/sample counter). `0xFFFF_FFFF` means host-armed, before
    ///   any invocation.
    pub fuel: u64,
    /// Trap code slot: [`TRAP_CODE_NONE`] (0) = no trap,
    /// [`TRAP_CODE_OUT_OF_FUEL`] (1) = out of fuel. Written by emitted
    /// fuel checks; read by the host after each guest entry.
    pub trap: u32,
    /// Per-instance globals/uniforms layout; may be null until wired up.
    /// Reserved as the future home for probe trace state — leave untouched.
    pub metadata: *const LpsType,
}

// SAFETY: `metadata` is an opaque pointer in the ABI header; hosts that share a `VmContext` across
// threads must synchronize updates. The type is plain data suitable for `repr(C)` layouts.
unsafe impl Send for VmContext {}
unsafe impl Sync for VmContext {}

impl Default for VmContext {
    fn default() -> Self {
        Self {
            fuel: DEFAULT_VMCTX_FUEL,
            trap: TRAP_CODE_NONE,
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

    // Note: Globals and uniforms access uses byte offsets (from LpsModuleSig layout)
    // rather than indexed accessors. Use LpvmDataQ32 with computed offsets for typed access.
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
            core::ptr::addr_of!(header.trap) as usize - base,
            VMCTX_OFFSET_TRAP
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
