//! LPVM trait implementation: CraneliftEngine

use lpir::lpir_module::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory};

use crate::compile_options::CompileOptions;
use crate::cranelift_host_memory::CraneliftHostMemory;
use crate::error::CompilerError;
use crate::lpvm_module::CraneliftModule;

/// Cranelift JIT engine implementing [`LpvmEngine`].
///
/// # Memory
///
/// Shared memory uses the **global allocator** ([`CraneliftHostMemory`]): `LpvmBuffer` carries the
/// same address as `guest_base` (single address space for JIT). Works on `no_std` + `alloc` targets
/// (e.g. ESP32) as long as a global allocator is registered — no fixed-size bump arena.
pub struct CraneliftEngine {
    options: CompileOptions,
    shared_memory: CraneliftHostMemory,
}

impl CraneliftEngine {
    /// Create a new Cranelift JIT engine with the given compile options.
    pub fn new(options: CompileOptions) -> Self {
        Self {
            options,
            shared_memory: CraneliftHostMemory::new(),
        }
    }
}

impl LpvmEngine for CraneliftEngine {
    type Module = CraneliftModule;
    type Error = CompilerError;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        CraneliftModule::compile(ir, meta, self.options.clone())
    }

    fn compile_with_config(
        &self,
        ir: &LpirModule,
        meta: &LpsModuleSig,
        config: &lpir::CompilerConfig,
    ) -> Result<Self::Module, Self::Error> {
        let mut opts = self.options.clone();
        opts.config = config.clone();
        opts.q32_options = config.q32;
        CraneliftModule::compile(ir, meta, opts)
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.shared_memory
    }
}
