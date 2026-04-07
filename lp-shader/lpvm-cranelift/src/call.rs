//! Level-1 [`LpsValueF64`] calls using [`lps_shared::LpsModuleSig`].

use cranelift_codegen::ir::ArgumentPurpose;
use lpir::FloatMode;
use lps_shared::LpsType;

use crate::jit_module::JitModule;
use lps_shared::lps_value_f64::{
    decode_q32_return, flatten_q32_arg, CallError, CallResult, GlslReturn, LpsValueF64,
};

impl JitModule {
    /// Typed Q32 call using GLSL metadata from lowering.
    pub fn call(&self, name: &str, args: &[LpsValueF64]) -> CallResult<GlslReturn<LpsValueF64>> {
        if self.float_mode != FloatMode::Q32 {
            return Err(CallError::Unsupported(
                "Level-1 GlslQ32 call requires FloatMode::Q32".into(),
            ));
        }
        let gfn = self
            .glsl_meta
            .functions
            .iter()
            .find(|f| f.name == name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        if gfn.parameters.len() != args.len() {
            return Err(CallError::Arity {
                expected: gfn.parameters.len(),
                got: args.len(),
            });
        }
        let idx = self
            .name_to_index
            .get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let param_count = self.ir_param_counts[*idx] as usize;
        let mut flat: alloc::vec::Vec<i32> = alloc::vec::Vec::new();
        for (p, a) in gfn.parameters.iter().zip(args.iter()) {
            flat.extend(flatten_q32_arg(p, a)?);
        }
        if flat.len() != param_count {
            return Err(CallError::Unsupported(alloc::format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            )));
        }
        let header = lpvm::VmContextHeader::default();
        let vmctx = core::ptr::from_ref(&header).cast::<u8>();
        let user = flat.as_slice();
        let sig = self
            .signatures
            .get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let uses_struct_return = sig
            .params
            .iter()
            .any(|p| p.purpose == ArgumentPurpose::StructReturn);
        let n_ret = self
            .logical_return_words
            .get(name)
            .copied()
            .unwrap_or_else(|| sig.returns.len());
        let code = self
            .finalized_ptr(name)
            .ok_or_else(|| CallError::Unsupported("internal: missing finalized pointer".into()))?;
        let words = unsafe {
            crate::invoke::invoke_i32_args_returns(code, vmctx, user, n_ret, uses_struct_return)?
        };
        if gfn.return_type == LpsType::Void {
            return Ok(GlslReturn {
                value: None,
                outs: alloc::vec::Vec::new(),
            });
        }
        let value = decode_q32_return(&gfn.return_type, &words)?;
        Ok(GlslReturn {
            value: Some(value),
            outs: alloc::vec::Vec::new(),
        })
    }
}
