//! [`LpvmModule`] for JIT code.

use alloc::sync::Arc;

use lpir::IrModule;
use lps_shared::LpsModuleSig;
use lpvm::{AllocError, LpvmMemory, LpvmModule};
use lpvm::{DEFAULT_VMCTX_FUEL, VMCTX_HEADER_SIZE};

use crate::error::NativeError;
use crate::native_options::NativeCompileOptions;

use super::buffer::JitBuffer;
use super::instance::NativeJitInstance;

pub(crate) struct NativeJitModuleInner {
    pub ir: IrModule,
    pub meta: LpsModuleSig,
    pub buffer: JitBuffer,
    pub entry_offsets: alloc::collections::BTreeMap<alloc::string::String, usize>,
    pub arena: Arc<lpvm::BumpLpvmMemory>,
    pub options: NativeCompileOptions,
}

/// JIT-compiled module (immutable after [`NativeJitEngine::compile`]).
#[derive(Clone)]
pub struct NativeJitModule {
    pub(crate) inner: Arc<NativeJitModuleInner>,
}

impl NativeJitModule {
    pub(crate) fn buffer(&self) -> &JitBuffer {
        &self.inner.buffer
    }

    pub(crate) fn entry_offset(&self, name: &str) -> Option<usize> {
        self.inner.entry_offsets.get(name).copied()
    }
}

impl LpvmModule for NativeJitModule {
    type Instance = NativeJitInstance;
    type Error = NativeError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.inner.meta
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        let align = 16usize;
        let size = VMCTX_HEADER_SIZE.max(align);
        let buf = self
            .inner
            .arena
            .alloc(size, align)
            .map_err(|e: AllocError| NativeError::Alloc(alloc::format!("{e:?}")))?;
        unsafe {
            let slot = core::slice::from_raw_parts_mut(buf.native_ptr(), VMCTX_HEADER_SIZE);
            write_vmctx_header(slot);
        }
        Ok(NativeJitInstance {
            module: self.clone(),
            vmctx_guest: buf.guest_base() as u32,
        })
    }
}

fn write_vmctx_header(out: &mut [u8]) {
    debug_assert!(out.len() >= VMCTX_HEADER_SIZE);
    out[0..8].copy_from_slice(&DEFAULT_VMCTX_FUEL.to_le_bytes());
    out[8..12].copy_from_slice(&0u32.to_le_bytes());
    out[12..16].copy_from_slice(&0u32.to_le_bytes());
}
