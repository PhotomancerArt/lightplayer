//! LPVM trait implementations: [`CraneliftInstance`] and [`lpvm::LpvmModule`] for [`CraneliftModule`].

use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use cranelift_codegen::ir::ArgumentPurpose;
use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, LpsValueQ32, ParamQualifier};
use lpvm::{
    CallError, LpsValueF32, LpvmInstance, LpvmModule, VmContext, decode_q32_return,
    encode_uniform_write, encode_uniform_write_q32, flat_q32_words_from_f32_args,
    glsl_component_count, q32_to_lps_value_f32,
};

use crate::lpvm_module::CraneliftModule;

/// Execution error for [`CraneliftInstance`].
#[derive(Debug)]
pub enum InstanceError {
    Call(CallError),
    Unsupported(&'static str),
}

impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceError::Call(e) => e.fmt(f),
            InstanceError::Unsupported(s) => write!(f, "{s}"),
        }
    }
}

impl From<CallError> for InstanceError {
    fn from(value: CallError) -> Self {
        InstanceError::Call(value)
    }
}

/// Per-instance VMContext storage and [`LpvmInstance`] over a finalized [`CraneliftModule`].
pub struct CraneliftInstance {
    module: Arc<CraneliftModule>,
    vmctx_buf: Vec<u8>,
    /// Byte offset from vmctx base to globals region
    globals_offset: usize,
    /// Byte offset from vmctx base to snapshot region
    snapshot_offset: usize,
    /// Size of globals region in bytes
    globals_size: usize,
}

impl CraneliftInstance {
    pub(crate) fn new(module: &CraneliftModule) -> Self {
        let meta = module.metadata();
        let total_size = meta.vmctx_buffer_size();
        let globals_offset = meta.globals_offset();
        let snapshot_offset = meta.snapshot_offset();
        let globals_size = meta.globals_size();

        let mut vmctx_buf = Vec::new();
        vmctx_buf.resize(total_size, 0);
        let header = VmContext::default();
        unsafe {
            core::ptr::write(vmctx_buf.as_mut_ptr().cast(), header);
        }

        let mut instance = Self {
            module: Arc::new(module.clone()),
            vmctx_buf,
            globals_offset,
            snapshot_offset,
            globals_size,
        };

        // Auto-init globals: call __shader_init if it exists, then snapshot
        let _ = instance.init_globals();

        instance
    }

    fn vmctx_ptr(&self) -> *const u8 {
        self.vmctx_buf.as_ptr()
    }

    /// Initialize globals by calling `__shader_init` if it exists,
    /// then memcpy globals -> snapshot to capture the initialized state.
    pub fn init_globals(&mut self) -> Result<(), InstanceError> {
        // Call __shader_init if it exists
        if self.module.has_function("__shader_init") {
            self.call_q32("__shader_init", &[])?;
        }

        // Copy globals region to snapshot region
        self.snapshot_globals();
        Ok(())
    }

    /// Reset globals by memcpy snapshot -> globals.
    /// This is a no-op if globals_size == 0.
    pub fn reset_globals(&mut self) {
        if self.globals_size == 0 {
            return;
        }

        let base = self.vmctx_buf.as_mut_ptr();
        let globals_ptr = unsafe { base.add(self.globals_offset) };
        let snapshot_ptr = unsafe { base.add(self.snapshot_offset) };

        unsafe {
            core::ptr::copy_nonoverlapping(snapshot_ptr, globals_ptr, self.globals_size);
        }
    }

    /// Copy globals region to snapshot region (for init).
    fn snapshot_globals(&mut self) {
        if self.globals_size == 0 {
            return;
        }

        let base = self.vmctx_buf.as_mut_ptr();
        let globals_ptr = unsafe { base.add(self.globals_offset) };
        let snapshot_ptr = unsafe { base.add(self.snapshot_offset) };

        unsafe {
            core::ptr::copy_nonoverlapping(globals_ptr, snapshot_ptr, self.globals_size);
        }
    }

    fn vmctx_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), InstanceError> {
        let end = offset.checked_add(data.len()).ok_or_else(|| {
            InstanceError::Call(CallError::Unsupported(String::from(
                "vmctx write: offset overflow",
            )))
        })?;
        if end > self.vmctx_buf.len() {
            return Err(InstanceError::Call(CallError::Unsupported(format!(
                "vmctx write out of bounds: end {end} len {}",
                self.vmctx_buf.len()
            ))));
        }
        self.vmctx_buf[offset..end].copy_from_slice(data);
        Ok(())
    }
}

impl LpvmModule for CraneliftModule {
    type Instance = CraneliftInstance;
    type Error = InstanceError;

    fn signatures(&self) -> &LpsModuleSig {
        self.metadata()
    }

    fn instantiate(&self) -> Result<Self::Instance, Self::Error> {
        Ok(CraneliftInstance::new(self))
    }
}

impl LpvmInstance for CraneliftInstance {
    type Error = InstanceError;

    fn call(&mut self, name: &str, args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        // Reset globals before each call to ensure fresh state
        self.reset_globals();

        if self.module.float_mode() != FloatMode::Q32 {
            return Err(InstanceError::Unsupported(
                "CraneliftInstance::call requires FloatMode::Q32; use direct_call for F32 JIT",
            ));
        }

        let gfn = self
            .module
            .metadata()
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let return_ty = gfn.return_type.clone();

        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))
                .into());
            }
        }

        if return_ty == LpsType::Void {
            return Err(InstanceError::Unsupported(
                "void return is not represented as LpsValue; use a typed return",
            ));
        }

        if gfn.parameters.len() != args.len() {
            return Err(CallError::Arity {
                expected: gfn.parameters.len(),
                got: args.len(),
            }
            .into());
        }

        let flat = flat_q32_words_from_f32_args(&gfn.parameters, args)?;
        let idx = *self
            .module
            .name_to_index()
            .get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let param_count = self.module.ir_param_counts()[idx] as usize;
        if flat.len() != param_count {
            return Err(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            ))
            .into());
        }

        let words = self.call_q32(name, &flat)?;
        let gq = decode_q32_return(&return_ty, &words)?;
        q32_to_lps_value_f32(&return_ty, gq)
            .map_err(|e| InstanceError::Call(CallError::TypeMismatch(e.to_string())))
    }

    fn call_q32(&mut self, name: &str, args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        // Reset globals before each call to ensure fresh state
        self.reset_globals();

        if self.module.float_mode() != FloatMode::Q32 {
            return Err(InstanceError::Unsupported(
                "CraneliftInstance::call_q32 requires FloatMode::Q32",
            ));
        }

        let gfn = self
            .module
            .metadata()
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;

        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))
                .into());
            }
        }

        let idx = *self
            .module
            .name_to_index()
            .get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let param_count = self.module.ir_param_counts()[idx] as usize;

        let expected_words: usize = gfn
            .parameters
            .iter()
            .map(|p| glsl_component_count(&p.ty))
            .sum();
        if args.len() != expected_words {
            return Err(CallError::Arity {
                expected: expected_words,
                got: args.len(),
            }
            .into());
        }
        if args.len() != param_count {
            return Err(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                args.len(),
                param_count
            ))
            .into());
        }

        let sig = self
            .module
            .signature(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);
        let n_ret = self
            .module
            .logical_return_words()
            .get(name)
            .copied()
            .unwrap_or_else(|| sig.returns.len());

        let code = self.module.code_ptr(name).ok_or_else(|| {
            CallError::Unsupported(String::from("internal: missing code pointer"))
        })?;

        let words = unsafe {
            crate::invoke::invoke_i32_args_returns(
                code,
                self.vmctx_ptr(),
                args,
                n_ret,
                uses_struct_return,
            )?
        };

        if gfn.return_type == LpsType::Void {
            return Ok(Vec::new());
        }

        Ok(words)
    }

    fn set_uniform(&mut self, path: &str, value: &LpsValueF32) -> Result<(), Self::Error> {
        let (off, bytes) = encode_uniform_write(
            self.module.metadata(),
            path,
            value,
            self.module.float_mode(),
        )
        .map_err(|e| InstanceError::Call(CallError::Unsupported(format!("set_uniform: {e}"))))?;
        self.vmctx_write_bytes(off, &bytes)
    }

    fn set_uniform_q32(&mut self, path: &str, value: &LpsValueQ32) -> Result<(), Self::Error> {
        let (off, bytes) =
            encode_uniform_write_q32(self.module.metadata(), path, value).map_err(|e| {
                InstanceError::Call(CallError::Unsupported(format!("set_uniform_q32: {e}")))
            })?;
        self.vmctx_write_bytes(off, &bytes)
    }
}
