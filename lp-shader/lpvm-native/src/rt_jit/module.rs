//! [`LpvmModule`] for JIT code.

use alloc::sync::Arc;

use alloc::string::String;
use lp_collection::VecMap;
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
    pub meta: LpsModuleSig,
    pub buffer: JitBuffer,
    pub entry_offsets: VecMap<alloc::string::String, usize>,
    pub entry_info: VecMap<alloc::string::String, NativeJitEntryInfo>,
    pub options: NativeCompileOptions,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeJitEntryInfo {
    pub arg_count: usize,
    pub ret_count: usize,
    pub is_sret: bool,
    pub supports_render_texture: bool,
    pub supports_render_samples: bool,
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
        let info = self.inner.entry_info.get(name)?;

        Some(NativeJitDirectCall {
            entry_offset,
            arg_count: info.arg_count,
            ret_count: info.ret_count,
            is_sret: info.is_sret,
        })
    }
}

impl LpvmModule for NativeJitModule {
    type Instance = NativeJitInstance;
    type Error = NativeError;

    fn signatures(&self) -> &LpsModuleSig {
        &self.inner.meta
    }

    /// The JIT always compiles for [`IsaTarget::native`] — the machine it is
    /// running on — so the target needs no field of its own here.
    fn float_impl(&self) -> lpvm::FloatImpl {
        IsaTarget::native().float_impl_for(self.inner.options.float_mode)
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
            render_samples_cache: None,
        };

        // Auto-init globals: call __shader_init if it exists, then snapshot
        instance.init_globals()?;

        Ok(instance)
    }

    fn code_size_bytes(&self) -> Option<usize> {
        Some(self.inner.buffer.len())
    }

    fn final_instruction_count(&self) -> Option<usize> {
        Some(self.inner.buffer.len() / 4)
    }
}

pub(crate) fn build_entry_info(
    ir: &LpirModule,
    meta: &LpsModuleSig,
    isa: IsaTarget,
) -> Result<VecMap<String, NativeJitEntryInfo>, NativeError> {
    let mut entries = VecMap::new();
    for ir_func in ir.functions.values() {
        let gfn = meta
            .functions
            .iter()
            .find(|f| f.name == ir_func.name)
            .ok_or_else(|| {
                NativeError::Internal(alloc::format!(
                    "missing module signature for function {}",
                    ir_func.name
                ))
            })?;
        let func_abi = match isa {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => crate::isa::rv32::abi::func_abi_rv32(gfn, Some(ir_func)),
            #[cfg(feature = "isa-xt")]
            IsaTarget::Xtensa => crate::isa::xt::abi::func_abi_xt(gfn, Some(ir_func)),
        };
        entries.insert(
            ir_func.name.clone(),
            NativeJitEntryInfo {
                arg_count: ir_func.param_count as usize,
                // NOT `ir_func.return_types.len()`. For an sret function that
                // is **zero** — `FunctionBuilder::finish` asserts sret
                // functions have empty `return_types`, because the results
                // leave through the caller-provided pointer instead. The real
                // count lives in the ABI, computed from the signature's return
                // type, and is the same number the emitter builds the prologue
                // from — so caller and callee cannot drift.
                //
                // Getting this wrong is not a cosmetic mismatch: `invoke_flat`
                // sizes its sret buffer `ret_count.max(1)` and the callee
                // writes `sret_word_count` words through the pointer, so a
                // zero here means a 1-word buffer receiving 4 words. `rt_emu`
                // has always sized from the return type (see
                // `run_emulator_call`'s `struct_size`); this is the JIT
                // catching up.
                ret_count: func_abi
                    .sret_word_count()
                    .map_or_else(|| ir_func.return_types.len(), |w| w as usize),
                is_sret: func_abi.is_sret(),
                supports_render_texture: lpvm::validate_render_texture_sig_ir(ir_func).is_ok(),
                supports_render_samples: lpvm::validate_render_samples_sig_ir(ir_func).is_ok(),
            },
        );
    }
    Ok(entries)
}

fn write_vmctx_header(out: &mut [u8]) {
    debug_assert!(out.len() >= VMCTX_HEADER_SIZE);
    out[0..8].copy_from_slice(&DEFAULT_VMCTX_FUEL.to_le_bytes());
    out[8..12].copy_from_slice(&0u32.to_le_bytes());
    out[12..16].copy_from_slice(&0u32.to_le_bytes());
}
