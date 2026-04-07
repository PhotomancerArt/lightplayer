//! [`lps_exec::GlslExecutable`] for `lpvm_cranelift::JitModule` (host JIT).

use std::collections::BTreeMap;

use lpir::FloatMode as LpirFloatMode;
use lps_diagnostics::GlslError;
use lps_exec::GlslExecutable;
use lps_shared::{LpsFnSig, LpsType};
use lpvm::LpsValue;
use lpvm_cranelift::{jit, CompileOptions, CompilerError, JitModule};
use lpvm_cranelift::{GlslReturn, LpsValueF64};

use super::q32_exec_common::{
    args_to_q32, call_array_from_q32, call_bool_from_q32, call_bvec_from_q32, call_f32_from_q32,
    call_i32_from_q32, call_ivec_from_q32, call_mat_from_q32, call_uvec_from_q32,
    call_vec_from_q32, impl_call_void, map_call_err, signatures_from_meta, Q32ShaderExecutable,
};

/// Host JIT executable for `jit.q32` / `jit.f32` filetest targets.
pub struct LpirJitExecutable {
    module: JitModule,
    signatures: BTreeMap<String, LpsFnSig>,
}

impl LpirJitExecutable {
    /// Compile GLSL with the LPIR JIT pipeline.
    pub fn try_new(
        source: &str,
        float_mode: crate::targets::FloatMode,
    ) -> Result<Self, CompilerError> {
        let fm = match float_mode {
            crate::targets::FloatMode::Q32 => LpirFloatMode::Q32,
            crate::targets::FloatMode::F32 => LpirFloatMode::F32,
        };
        let options = CompileOptions {
            float_mode: fm,
            ..Default::default()
        };
        let module = jit(source, &options)?;
        let signatures = signatures_from_meta(module.glsl_meta());
        Ok(Self { module, signatures })
    }

    fn gfn_meta(&self, name: &str) -> Option<&LpsFnSig> {
        self.module
            .glsl_meta()
            .functions
            .iter()
            .find(|f| f.name == name)
    }
}

impl Q32ShaderExecutable for LpirJitExecutable {
    fn call_q32_ret(
        &mut self,
        name: &str,
        args: &[LpsValue],
    ) -> Result<GlslReturn<LpsValueF64>, GlslError> {
        let gfn = self.gfn_meta(name).ok_or_else(|| {
            GlslError::new(
                lps_diagnostics::ErrorCode::E0101,
                format!("function '{name}' not found"),
            )
        })?;
        let qargs = args_to_q32(gfn, args)?;
        self.module.call(name, &qargs).map_err(map_call_err)
    }

    fn signatures_map(&self) -> &BTreeMap<String, LpsFnSig> {
        &self.signatures
    }
}

impl GlslExecutable for LpirJitExecutable {
    fn call_void(&mut self, name: &str, args: &[LpsValue]) -> Result<(), GlslError> {
        impl_call_void(self, name, args)
    }

    fn call_i32(&mut self, name: &str, args: &[LpsValue]) -> Result<i32, GlslError> {
        call_i32_from_q32(self, name, args)
    }

    fn call_f32(&mut self, name: &str, args: &[LpsValue]) -> Result<f32, GlslError> {
        call_f32_from_q32(self, name, args)
    }

    fn call_bool(&mut self, name: &str, args: &[LpsValue]) -> Result<bool, GlslError> {
        call_bool_from_q32(self, name, args)
    }

    fn call_bvec(
        &mut self,
        name: &str,
        args: &[LpsValue],
        dim: usize,
    ) -> Result<Vec<bool>, GlslError> {
        call_bvec_from_q32(self, name, args, dim)
    }

    fn call_ivec(
        &mut self,
        name: &str,
        args: &[LpsValue],
        dim: usize,
    ) -> Result<Vec<i32>, GlslError> {
        call_ivec_from_q32(self, name, args, dim)
    }

    fn call_uvec(
        &mut self,
        name: &str,
        args: &[LpsValue],
        dim: usize,
    ) -> Result<Vec<u32>, GlslError> {
        call_uvec_from_q32(self, name, args, dim)
    }

    fn call_vec(
        &mut self,
        name: &str,
        args: &[LpsValue],
        dim: usize,
    ) -> Result<Vec<f32>, GlslError> {
        call_vec_from_q32(self, name, args, dim)
    }

    fn call_mat(
        &mut self,
        name: &str,
        args: &[LpsValue],
        rows: usize,
        cols: usize,
    ) -> Result<Vec<f32>, GlslError> {
        call_mat_from_q32(self, name, args, rows, cols)
    }

    fn call_array(
        &mut self,
        name: &str,
        args: &[LpsValue],
        elem_ty: &LpsType,
        len: usize,
    ) -> Result<Vec<LpsValue>, GlslError> {
        call_array_from_q32(self, name, args, elem_ty, len)
    }

    fn get_function_signature(&self, name: &str) -> Option<&LpsFnSig> {
        self.signatures.get(name)
    }

    fn list_functions(&self) -> Vec<String> {
        self.module.func_names().to_vec()
    }
}
