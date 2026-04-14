//! [`EmuModule`]: linked RV32 image + shared arena handle.

use alloc::sync::Arc;

use lp_riscv_elf::ElfLoadInfo;
use lpir::lpir_module::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmModule, ModuleDebugInfo};
use lpvm_cranelift::CompileOptions;

use crate::instance::{EmuInstance, InstanceError};
use crate::memory::EmuSharedArena;

/// Compiled RV32 module for the LPVM emulator (immutable after [`lpvm::LpvmEngine::compile`]).
#[derive(Clone)]
pub struct EmuModule {
    pub(crate) ir: LpirModule,
    pub(crate) meta: LpsModuleSig,
    pub(crate) load: Arc<ElfLoadInfo>,
    pub(crate) options: CompileOptions,
    pub(crate) arena: EmuSharedArena,
    /// Debug info with disasm sections.
    pub(crate) debug_info: ModuleDebugInfo,
}

impl LpvmModule for EmuModule {
    type Instance = EmuInstance;
    type Error = InstanceError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.meta
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        EmuInstance::new(self.clone())
    }

    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        Some(&self.debug_info)
    }
}
