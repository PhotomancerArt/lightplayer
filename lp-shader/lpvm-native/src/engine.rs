//! [`LpvmEngine`] implementation (compile is M2+).

use alloc::string::String;

use lpir::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{BumpLpvmMemory, LpvmEngine, LpvmMemory};

use crate::error::NativeError;
use crate::module::NativeModule;

/// Backend-specific compile options (not shared with Cranelift / WASM).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeCompileOptions {
    pub float_mode: lpir::FloatMode,
}

impl Default for NativeCompileOptions {
    fn default() -> Self {
        Self {
            float_mode: lpir::FloatMode::Q32,
        }
    }
}

/// Default bump arena size for shared memory until firmware wires a real region.
const DEFAULT_BUMP_BYTES: usize = 64 * 1024;

/// Native code generator engine (stub compile in M1).
pub struct NativeEngine {
    pub options: NativeCompileOptions,
    memory: BumpLpvmMemory,
}

impl NativeEngine {
    pub fn new(options: NativeCompileOptions) -> Self {
        Self {
            options,
            memory: BumpLpvmMemory::new(DEFAULT_BUMP_BYTES),
        }
    }
}

impl LpvmEngine for NativeEngine {
    type Module = NativeModule;
    type Error = NativeError;

    fn compile(&self, _ir: &IrModule, _meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        Err(NativeError::NotYetImplemented(String::from(
            "M2: lower, regalloc, emit",
        )))
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.memory
    }
}

#[cfg(test)]
mod tests {
    use lpir::IrModule;
    use lps_shared::LpsModuleSig;

    use super::*;

    #[test]
    fn compile_returns_not_yet_implemented() {
        let engine = NativeEngine::new(NativeCompileOptions::default());
        let ir = IrModule::default();
        let meta = LpsModuleSig::default();
        let err = engine.compile(&ir, &meta).expect_err("M1 stub");
        match err {
            NativeError::NotYetImplemented(s) => assert!(s.contains("M2")),
            e => panic!("unexpected {e:?}"),
        }
    }

    #[test]
    fn memory_returns_bump() {
        let engine = NativeEngine::new(NativeCompileOptions::default());
        let _ = engine.memory();
    }
}
