//! [`EmuEngine`]: compile to linked RV32 + shared [`EmuSharedArena`].

use lpir::lpir_module::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory, ModuleDebugInfo};
use lpvm_cranelift::CompileOptions;
use lpvm_cranelift::link_object_with_builtins;
use lpvm_cranelift::{CompilerError, object_bytes_from_ir};

use crate::memory::{DEFAULT_SHARED_CAPACITY, EmuSharedArena};
use crate::module::EmuModule;

/// LPVM engine targeting the in-process RV32 emulator.
pub struct EmuEngine {
    options: CompileOptions,
    arena: EmuSharedArena,
}

impl EmuEngine {
    pub fn new(options: CompileOptions) -> Self {
        Self {
            options,
            arena: EmuSharedArena::new(DEFAULT_SHARED_CAPACITY),
        }
    }

    pub fn options(&self) -> CompileOptions {
        self.options.clone()
    }
}

impl LpvmEngine for EmuEngine {
    type Module = EmuModule;
    type Error = CompilerError;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let object = object_bytes_from_ir(ir, &self.options)?;
        let load = alloc::sync::Arc::new(link_object_with_builtins(&object)?);
        Ok(EmuModule {
            ir: ir.clone(),
            meta: meta.clone(),
            load,
            options: self.options.clone(),
            arena: self.arena.clone(),
            // Cranelift-based backends don't generate interleaved debug info;
            // disasm available via external tools
            debug_info: ModuleDebugInfo::new(),
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.arena
    }
}
