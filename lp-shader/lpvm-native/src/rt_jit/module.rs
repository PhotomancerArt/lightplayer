//! [`LpvmModule`] for JIT code.

use alloc::sync::Arc;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{AllocError, LpvmMemory, LpvmModule};
use lpvm::{DEFAULT_VMCTX_FUEL, VMCTX_HEADER_SIZE};

use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::native_options::NativeCompileOptions;

use super::buffer::JitBuffer;
use super::host_memory::NativeHostMemory;
use super::instance::NativeJitInstance;

pub(crate) struct NativeJitModuleInner {
    pub ir: LpirModule,
    pub meta: LpsModuleSig,
    pub buffer: JitBuffer,
    pub entry_offsets: alloc::collections::BTreeMap<alloc::string::String, usize>,
    pub options: NativeCompileOptions,
    pub isa: IsaTarget,
}

/// Cached function handle for fast calls (like cranelift's `DirectCall`).
///
/// Created once at compile time via [`NativeJitModule::direct_call`],
/// then reused for zero-overhead per-pixel calls.
#[derive(Clone, Copy, Debug)]
pub struct NativeJitDirectCall {
    pub(crate) entry_offset: usize,
    pub(crate) arg_count: usize,
    pub(crate) ret_count: usize,
    pub(crate) is_sret: bool,
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

    /// Create a cached function handle for fast direct calls.
    ///
    /// This resolves the function index, entry offset, and ABI info at compile time,
    /// eliminating per-call string lookups and metadata searches.
    pub fn direct_call(&self, name: &str) -> Option<NativeJitDirectCall> {
        let entry_offset = self.inner.entry_offsets.get(name).copied()?;

        let ir_func = self.inner.ir.functions.values().find(|f| f.name == name)?;

        let gfn = self.inner.meta.functions.iter().find(|f| f.name == name)?;

        let slots = ir_func.total_param_slots() as usize;
        let func_abi = match self.inner.isa {
            IsaTarget::Rv32imac => crate::isa::rv32::abi::func_abi_rv32(gfn, slots),
        };

        Some(NativeJitDirectCall {
            entry_offset,
            arg_count: ir_func.param_count as usize,
            ret_count: ir_func.return_types.len(),
            is_sret: func_abi.is_sret(),
        })
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
        let total_size = self.inner.meta.vmctx_buffer_size();
        let size = total_size.max(align);
        let memory = NativeHostMemory::new();
        let buf = memory
            .alloc(size, align)
            .map_err(|e: AllocError| NativeError::Alloc(alloc::format!("{e:?}")))?;

        // Zero-initialize the entire buffer, then write the vmctx header
        unsafe {
            let slot = core::slice::from_raw_parts_mut(buf.native_ptr(), total_size);
            slot.fill(0);
            write_vmctx_header(&mut slot[..VMCTX_HEADER_SIZE]);
        }

        let globals_offset = self.inner.meta.globals_offset() as u32;
        let snapshot_offset = self.inner.meta.snapshot_offset() as u32;
        let globals_size = self.inner.meta.globals_size() as u32;

        let mut instance = NativeJitInstance {
            module: self.clone(),
            vmctx_guest: buf.guest_base() as u32,
            globals_offset,
            snapshot_offset,
            globals_size,
            render_texture_cache: None,
        };

        // Auto-init globals: call __shader_init if it exists, then snapshot
        instance.init_globals()?;

        Ok(instance)
    }
}

fn write_vmctx_header(out: &mut [u8]) {
    debug_assert!(out.len() >= VMCTX_HEADER_SIZE);
    out[0..8].copy_from_slice(&DEFAULT_VMCTX_FUEL.to_le_bytes());
    out[8..12].copy_from_slice(&0u32.to_le_bytes());
    out[12..16].copy_from_slice(&0u32.to_le_bytes());
}
