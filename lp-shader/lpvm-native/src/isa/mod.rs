//! ISA abstraction (one backend per target family).

pub mod rv32;
pub mod rv32fa;

use alloc::string::String;
use alloc::vec::Vec;

use crate::regalloc::Allocation;
use crate::vinst::VInst;

/// Emitted code + metadata (M2).
#[derive(Debug, Default)]
pub struct CodeBlob {
    pub bytes: Vec<u8>,
}

/// Lower VInst + allocation to machine code (stub in M1).
pub trait IsaBackend {
    fn emit_function(&self, _vinsts: &[VInst], _alloc: &Allocation) -> Result<CodeBlob, String> {
        Err(String::from(
            "M2: IsaBackend::emit_function not implemented",
        ))
    }
}

/// RV32 backend placeholder.
#[derive(Debug, Default)]
pub struct Rv32Backend;

impl IsaBackend for Rv32Backend {}
