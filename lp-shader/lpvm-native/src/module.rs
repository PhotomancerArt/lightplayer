//! [`LpvmModule`] — compiled artifact (M3 will hold code bytes).

use alloc::string::String;

use lps_shared::LpsModuleSig;
use lpvm::LpvmModule;

use crate::error::NativeError;
use crate::instance::NativeInstance;

/// Compiled module placeholder.
#[derive(Debug)]
pub struct NativeModule {
    signatures: LpsModuleSig,
}

impl NativeModule {
    /// Construct a module shell (for tests; [`NativeEngine::compile`] returns `Err` until M2).
    pub fn new_for_test(signatures: LpsModuleSig) -> Self {
        Self { signatures }
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
