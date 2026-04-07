//! [`EmuEngine`]: compile to linked RV32 + shared [`EmuSharedArena`].

use lpir::module::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory};
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
        self.options
    }
}

impl LpvmEngine for EmuEngine {
    type Module = EmuModule;
    type Error = CompilerError;

    fn compile(&self, ir: &IrModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        let object = object_bytes_from_ir(ir, &self.options)?;
        let load = alloc::sync::Arc::new(link_object_with_builtins(&object)?);
        Ok(EmuModule {
            ir: ir.clone(),
            meta: meta.clone(),
            load,
            options: self.options,
            arena: self.arena.clone(),
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.arena
    }
}
