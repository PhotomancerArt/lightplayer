//! [`LpvmModule`] — compiled ELF object (`.o`) for linking / emulation.

use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::LpsModuleSig;
use lpvm::LpvmModule;

use crate::error::NativeError;
use crate::instance::NativeInstance;

/// Compiled module: RV32 ELF relocatable object + signature metadata.
#[derive(Debug)]
pub struct NativeModule {
    /// ELF object bytes (`.o`), may contain multiple `.text` symbols.
    pub elf: Vec<u8>,
    signatures: LpsModuleSig,
}

impl NativeModule {
    /// Construct a module shell (for tests).
    pub fn new_for_test(signatures: LpsModuleSig) -> Self {
        Self {
            elf: Vec::new(),
            signatures,
        }
    }

    pub(crate) fn from_parts(elf: Vec<u8>, signatures: LpsModuleSig) -> Self {
        Self { elf, signatures }
    }
}

impl LpvmModule for NativeModule {
    type Instance = NativeInstance;
    type Error = NativeError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.signatures
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        Err(NativeError::NotYetImplemented(String::from(
            "M3: instantiate",
        )))
    }
}
