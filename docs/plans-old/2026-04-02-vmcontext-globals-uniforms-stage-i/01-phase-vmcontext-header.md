# Phase 1: Define VmContextHeader and Constants

## Scope of Phase

Create the `VmContextHeader` struct and offset constants in `lpvm`. This is the foundation
that all other components will reference.

## Code Organization Reminders

- Place struct definition and constants at the top of the file
- Helper functions (like `minimal()`) at the bottom
- Use `#[repr(C)]` for stable layout

## Implementation Details

### 1. Create `lpvm/src/vmcontext.rs`

```rust
//! VMContext header definition and utilities.

/// Offset of `fuel` field in VmContextHeader
pub const VMCTX_OFFSET_FUEL: usize = 0;
/// Offset of `trap_handler` field in VmContextHeader
pub const VMCTX_OFFSET_TRAP_HANDLER: usize = 8;
/// Offset of `globals_defaults_offset` field in VmContextHeader
pub const VMCTX_OFFSET_GLOBALS_DEFAULTS_OFFSET: usize = 12;
/// Total size of VmContextHeader in bytes
pub const VMCTX_HEADER_SIZE: usize = 16;

/// Well-known header at the start of every VMContext.
/// Fields are accessed by the host via these fixed offsets.
#[repr(C)]
pub struct VmContextHeader {
    /// Optional gas metering (u64 at offset 0)
    pub fuel: u64,
    /// Optional callback pointer (u32 at offset 8)
    pub trap_handler: u32,
    /// Offset to globals_defaults section (u32 at offset 12)
    pub globals_defaults_offset: u32,
}

impl VmContextHeader {
    /// Create a header with default values.
    pub fn new() -> Self {
        Self {
            fuel: 0,
            trap_handler: 0,
            globals_defaults_offset: 0,
        }
    }
}

/// Create a minimal VMContext for tests (header only, no uniforms/globals).
/// Returns a boxed byte array suitable for passing to shaders.
pub fn minimal_vmcontext() -> alloc::boxed::Box<[u8]> {
    let header = VmContextHeader::new();
    let bytes = unsafe {
        core::slice::from_raw_parts(
            &header as *const _ as *const u8,
            core::mem::size_of::<VmContextHeader>()
        )
    };
    bytes.into()
}
```

### 2. Update `lpvm/src/lib.rs`

Add re-exports:

```rust
pub mod vmcontext;
pub use vmcontext::{VmContextHeader, minimal_vmcontext};
pub use vmcontext::{VMCTX_OFFSET_FUEL, VMCTX_OFFSET_TRAP_HANDLER, VMCTX_OFFSET_GLOBALS_DEFAULTS_OFFSET, VMCTX_HEADER_SIZE};
```

## Tests to Write

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_size_is_16() {
        assert_eq!(core::mem::size_of::<VmContextHeader>(), 16);
    }

    #[test]
    fn field_offsets_correct() {
        let header = VmContextHeader::new();
        let base = &header as *const _ as usize;
        
        assert_eq!(&header.fuel as *const _ as usize - base, VMCTX_OFFSET_FUEL);
        assert_eq!(&header.trap_handler as *const _ as usize - base, VMCTX_OFFSET_TRAP_HANDLER);
        assert_eq!(&header.globals_defaults_offset as *const _ as usize - base, VMCTX_OFFSET_GLOBALS_DEFAULTS_OFFSET);
    }

    #[test]
    fn minimal_vmcontext_has_header_size() {
        let vmctx = minimal_vmcontext();
        assert_eq!(vmctx.len(), VMCTX_HEADER_SIZE);
    }
}
```

## Validate

```bash
cargo test -p lpvm
cargo check -p lpvm --target riscv32imac-unknown-none-elf
```
