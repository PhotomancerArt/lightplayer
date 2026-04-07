//! LPVM trait implementations: [`CraneliftInstance`] and [`lpvm::LpvmModule`] for [`CraneliftModule`].

use alloc::format;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::fmt;

use cranelift_codegen::ir::ArgumentPurpose;
use lpir::FloatMode;
use lps_shared::{LpsModuleSig, LpsType, ParamQualifier};
use lpvm::{LpsValue, LpvmInstance, LpvmModule, VMCTX_HEADER_SIZE, VmContext};

use crate::lpvm_module::CraneliftModule;
use lps_shared::q32::q32_value::{CallError, decode_q32_return, flatten_q32_arg};

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
}

impl CraneliftInstance {
    pub(crate) fn new(module: &CraneliftModule) -> Self {
        let mut vmctx_buf = Vec::new();
        vmctx_buf.resize(VMCTX_HEADER_SIZE, 0);
        let header = VmContext::default();
        unsafe {
            core::ptr::write(vmctx_buf.as_mut_ptr().cast(), header);
        }
        Self {
            module: Arc::new(module.clone()),
            vmctx_buf,
        }
    }

    fn vmctx_ptr(&self) -> *const u8 {
        self.vmctx_buf.as_ptr()
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

    fn call(&mut self, name: &str, args: &[LpsValue]) -> Result<LpsValue, Self::Error> {
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

        for p in &gfn.parameters {
            if matches!(p.qualifier, ParamQualifier::Out | ParamQualifier::InOut) {
                return Err(CallError::Unsupported(String::from(
                    "out/inout parameters are not supported for direct calling.",
                ))
                .into());
            }
        }

        if gfn.return_type == LpsType::Void {
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

        let idx = *self
            .module
            .name_to_index
            .get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let param_count = self.module.ir_param_counts[idx] as usize;

        let mut flat: Vec<i32> = Vec::new();
        for (p, a) in gfn.parameters.iter().zip(args.iter()) {
            let q = lps_shared::q32::q32_marshal::lps_value_to_glsl_q32(&p.ty, a)?;
            flat.extend(flatten_q32_arg(p, &q)?);
        }
        if flat.len() != param_count {
            return Err(CallError::Unsupported(format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
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
            .logical_return_words
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
                flat.as_slice(),
                n_ret,
                uses_struct_return,
            )?
        };

        let gq = decode_q32_return(&gfn.return_type, &words)?;
        lps_shared::q32::q32_marshal::glsl_q32_to_lps_value(&gfn.return_type, gq)
            .map_err(InstanceError::Call)
    }
}
