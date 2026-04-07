//! LPVM trait implementation: CraneliftEngine

use lpir::module::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{BumpLpvmMemory, LpvmEngine, LpvmMemory};

use crate::compile_options::CompileOptions;
use crate::error::CompilerError;
use crate::lpvm_module::CraneliftModule;

/// Default shared-memory arena size for [`CraneliftEngine`] (host bump heap until LPVM2 wires JIT memory).
const DEFAULT_LPVM_SHARED_MEMORY_BYTES: usize = 256 * 1024;

/// Cranelift JIT engine implementing [`LpvmEngine`].
///
/// This is the new trait-based API for LPVM compilation. It coexists with
/// the existing [`crate::jit_module::JitModule`] API until M7 (migration complete).
pub struct CraneliftEngine {
    options: CompileOptions,
    shared_memory: BumpLpvmMemory,
}

impl CraneliftEngine {
    /// Create a new Cranelift JIT engine with the given compile options.
    pub fn new(options: CompileOptions) -> Self {
        Self {
            options,
            shared_memory: BumpLpvmMemory::new(DEFAULT_LPVM_SHARED_MEMORY_BYTES),
        }
    }
}

impl LpvmEngine for CraneliftEngine {
    type Module = CraneliftModule;
    type Error = CompilerError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        CraneliftModule::compile(ir, meta, self.options)
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.shared_memory
    }
}
