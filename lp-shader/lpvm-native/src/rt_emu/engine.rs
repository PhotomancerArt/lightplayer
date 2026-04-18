//! [`LpvmEngine`] implementation for native RV32 → linked → emulated execution.

use alloc::sync::Arc;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory, ModuleDebugInfo};
use lpvm_emu::EmuSharedArena;

use crate::compile::compile_module;
use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::link::link_elf;
use crate::native_options::NativeCompileOptions;

use super::NativeEmuModule;

/// Engine that compiles LPIR to RV32, links with builtins, and emulates execution.
pub struct NativeEmuEngine {
    options: NativeCompileOptions,
    arena: EmuSharedArena,
}

impl NativeEmuEngine {
    /// Create new emulation engine with default shared memory capacity.
    pub fn new(options: NativeCompileOptions) -> Self {
        Self {
            options,
            arena: EmuSharedArena::new(lpvm_emu::DEFAULT_SHARED_CAPACITY),
        }
    }
}

impl LpvmEngine for NativeEmuEngine {
    type Module = NativeEmuModule;
    type Error = NativeError;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        // 1. Compile module
        let opts = self.options.clone();
        let compiled = compile_module(ir, meta, opts.float_mode, opts, IsaTarget::Rv32imac)?;

        // 2. Build ModuleDebugInfo from compiled functions
        let mut debug_info = ModuleDebugInfo::new();
        for func in &compiled.functions {
            debug_info.add_function(func.debug_info.clone());
        }

        // 3. Link to ELF
        let elf = link_elf(&compiled, IsaTarget::Rv32imac)
            .map_err(|e| NativeError::Internal(format!("ELF link failed: {e}")))?;

        // 4. Link with cranelift builtins
        let load = Arc::new(lpvm_cranelift::link_object_with_builtins(&elf)?);

        Ok(NativeEmuModule {
            ir: ir.clone(),
            _elf: elf,
            meta: meta.clone(),
            load,
            arena: self.arena.clone(),
            options: self.options.clone(),
            debug_info,
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.arena
    }
}
