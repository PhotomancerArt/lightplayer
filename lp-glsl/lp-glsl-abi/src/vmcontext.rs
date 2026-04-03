//! Fixed-layout header at the start of every VMContext allocation.

use alloc::boxed::Box;

/// Byte offset of [`VmContextHeader::fuel`].
pub const VMCTX_OFFSET_FUEL: usize = 0;
/// Byte offset of [`VmContextHeader::trap_handler`].
pub const VMCTX_OFFSET_TRAP_HANDLER: usize = 8;
/// Byte offset of [`VmContextHeader::globals_defaults_offset`].
pub const VMCTX_OFFSET_GLOBALS_DEFAULTS_OFFSET: usize = 12;
/// Size of [`VmContextHeader`] in bytes.
pub const VMCTX_HEADER_SIZE: usize = core::mem::size_of::<VmContextHeader>();

/// Well-known fields at the start of every VMContext (single flat allocation).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VmContextHeader {
    pub fuel: u64,
    pub trap_handler: u32,
    pub globals_defaults_offset: u32,
}

/// Allocate a zeroed header-sized buffer for tests and callers that do not use uniforms/globals yet.
pub fn minimal_vmcontext() -> Box<[u8]> {
    let header = VmContextHeader::default();
    let bytes = unsafe {
        core::slice::from_raw_parts(
            &header as *const VmContextHeader as *const u8,
            core::mem::size_of::<VmContextHeader>(),
        )
    };
    bytes.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_is_16() {
        assert_eq!(core::mem::size_of::<VmContextHeader>(), 16);
    }

    #[test]
    fn field_offsets_match_constants() {
        let header = VmContextHeader::default();
        let base = &header as *const VmContextHeader as usize;
        assert_eq!(
            &header.fuel as *const u64 as usize - base,
            VMCTX_OFFSET_FUEL
        );
        assert_eq!(
            &header.trap_handler as *const u32 as usize - base,
            VMCTX_OFFSET_TRAP_HANDLER
        );
        assert_eq!(
            &header.globals_defaults_offset as *const u32 as usize - base,
            VMCTX_OFFSET_GLOBALS_DEFAULTS_OFFSET
        );
    }

    #[test]
    fn minimal_vmcontext_len() {
        let b = minimal_vmcontext();
        assert_eq!(b.len(), VMCTX_HEADER_SIZE);
    }
}
