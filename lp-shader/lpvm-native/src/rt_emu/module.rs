//! [`LpvmModule`] implementation for linked + emulated native RV32.

use alloc::sync::Arc;
use alloc::vec::Vec;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmMemory, LpvmModule, ModuleDebugInfo};
use lpvm_emu::{EmuSharedArena, GUEST_VMCTX_BYTES, write_guest_vmctx_header};

use crate::error::NativeError;
use crate::native_options::NativeCompileOptions;

use super::NativeEmuInstance;

/// Compiled and linked module ready for emulation.
#[derive(Clone)]
pub struct NativeEmuModule {
    pub(crate) ir: LpirModule,
    /// Object bytes retained for debugging; not used at runtime.
    pub(crate) _elf: Vec<u8>,
    pub(crate) meta: LpsModuleSig,
    pub(crate) load: Arc<lp_riscv_elf::ElfLoadInfo>,
    pub(crate) arena: EmuSharedArena,
    pub(crate) options: NativeCompileOptions,
    /// Debug info with disasm sections.
    pub(crate) debug_info: ModuleDebugInfo,
}

impl LpvmModule for NativeEmuModule {
    type Instance = NativeEmuInstance;
    type Error = NativeError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.meta
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        use lpvm::AllocError;

        let align = 16usize;
        let size = GUEST_VMCTX_BYTES.max(align);
        let buf = self
            .arena
            .alloc(size, align)
            .map_err(|e: AllocError| NativeError::Alloc(alloc::format!("{e:?}")))?;
        unsafe {
            let slot = core::slice::from_raw_parts_mut(buf.native_ptr(), GUEST_VMCTX_BYTES);
            write_guest_vmctx_header(slot);
        }
        Ok(NativeEmuInstance {
            module: self.clone(),
            vmctx_guest: buf.guest_base() as u32,
            last_debug: None,
            last_guest_instruction_count: None,
        })
    }

    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        Some(&self.debug_info)
    }

    fn lpir_module(&self) -> Option<&LpirModule> {
        Some(&self.ir)
    }
}
