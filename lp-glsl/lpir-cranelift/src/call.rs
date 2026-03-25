//! Level-1 [`GlslQ32`] calls using [`lpir::GlslModuleMeta`].

use lpir::FloatMode;
use lpir::GlslType;

use crate::jit_module::JitModule;
use crate::values::{
    CallError, CallResult, GlslQ32, GlslReturn, decode_q32_return, flatten_q32_arg,
};

impl JitModule {
    /// Typed Q32 call using GLSL metadata from lowering.
    pub fn call(&self, name: &str, args: &[GlslQ32]) -> CallResult<GlslReturn<GlslQ32>> {
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
        if gfn.params.len() != args.len() {
            return Err(CallError::Arity {
                expected: gfn.params.len(),
                got: args.len(),
            });
        }
        let idx = self
            .name_to_index
            .get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?;
        let param_count = self.ir_param_counts[*idx] as usize;
        let mut flat: alloc::vec::Vec<i32> = alloc::vec::Vec::new();
        for (p, a) in gfn.params.iter().zip(args.iter()) {
            flat.extend(flatten_q32_arg(p, a)?);
        }
        if flat.len() != param_count {
            return Err(CallError::Unsupported(alloc::format!(
                "flattened argument count {} does not match IR param_count {}",
                flat.len(),
                param_count
            )));
        }
        let n_ret = self
            .signatures
            .get(name)
            .ok_or_else(|| CallError::MissingMetadata(name.into()))?
            .returns
            .len();
        let code = self
            .finalized_ptr(name)
            .ok_or_else(|| CallError::Unsupported("internal: missing finalized pointer".into()))?;
        let words = unsafe { crate::invoke::invoke_i32_args_returns(code, &flat, n_ret)? };
        if gfn.return_type == GlslType::Void {
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
