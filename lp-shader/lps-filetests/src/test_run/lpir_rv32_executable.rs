//! [`lps_exec::GlslExecutable`] via RV32 object + linked builtins + emulator.

use std::collections::BTreeMap;

use lp_riscv_elf::ElfLoadInfo;
use lpir::{FloatMode as LpirFloatMode, IrModule};
use lps_diagnostics::GlslError;
use lps_exec::GlslExecutable;
use lps_shared::{LpsFnSig, LpsModuleSig, LpsType};
use lpvm::LpsValue;
use lpvm_cranelift::{
    link_object_with_builtins, object_bytes_from_ir, CompileOptions, CompilerError,
};
use lpvm_cranelift::{GlslReturn, LpsValueF64};
use lpvm_emu::glsl_q32_call_emulated;

use super::q32_exec_common::{
    args_to_q32, call_array_from_q32, call_bool_from_q32, call_bvec_from_q32, call_f32_from_q32,
    call_i32_from_q32, call_ivec_from_q32, call_mat_from_q32, call_uvec_from_q32,
    call_vec_from_q32, impl_call_void, map_call_err, signatures_from_meta, Q32ShaderExecutable,
};

/// RV32 emulator-backed executable for `rv32.q32` filetests.
pub struct LpirRv32Executable {
    ir: IrModule,
    meta: LpsModuleSig,
    options: CompileOptions,
    load: ElfLoadInfo,
    signatures: BTreeMap<String, LpsFnSig>,
}

impl LpirRv32Executable {
    /// Compile GLSL, emit relocatable object, link with builtins ELF, load for emulation.
    pub fn try_new(
        source: &str,
        float_mode: crate::targets::FloatMode,
    ) -> Result<Self, CompilerError> {
        let naga =
            lps_frontend::compile(source).map_err(|e| CompilerError::Parse(format!("{e}")))?;
        let (ir, meta) = lps_frontend::lower(&naga).map_err(CompilerError::Lower)?;
        let fm = match float_mode {
            crate::targets::FloatMode::Q32 => LpirFloatMode::Q32,
            crate::targets::FloatMode::F32 => LpirFloatMode::F32,
        };
        let options = CompileOptions {
            float_mode: fm,
            ..Default::default()
        };
        let object = object_bytes_from_ir(&ir, &options)?;
        let load = link_object_with_builtins(&object)?;
        let signatures = signatures_from_meta(&meta);
        Ok(Self {
            ir,
            meta,
            options,
            load,
            signatures,
        })
    }

    fn gfn_meta(&self, name: &str) -> Option<&LpsFnSig> {
        self.meta.functions.iter().find(|f| f.name == name)
    }
}

impl Q32ShaderExecutable for LpirRv32Executable {
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
        glsl_q32_call_emulated(
            &self.load,
            &self.ir,
            &self.meta,
            &self.options,
            name,
            &qargs,
        )
        .map_err(map_call_err)
    }

    fn signatures_map(&self) -> &BTreeMap<String, LpsFnSig> {
        &self.signatures
    }
}

impl GlslExecutable for LpirRv32Executable {
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
        self.meta.functions.iter().map(|f| f.name.clone()).collect()
    }
}
