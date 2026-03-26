//! JIT builtin symbols and LPIR import resolution.
//!
//! Cranelift [`Signature`] and [`get_function_pointer`] for each [`BuiltinId`] are generated in
//! [`crate::generated_builtin_abi`] from the same `rust_signature` strings as
//! `lp-glsl-cranelift` `registry.rs` (`lp-glsl-builtins-gen-app`). Re-run codegen after changing any
//! `extern "C"` builtin in `lp-glsl-builtins`.

use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use cranelift_codegen::ir::{Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_module::{FuncId, Linkage, Module};
use lp_glsl_builtin_ids::{
    BuiltinId, GlslParamKind, glsl_lpfx_q32_builtin_id, glsl_q32_math_builtin_id,
    lpir_q32_builtin_id,
};
use lpir::FloatMode;
use lpir::module::{ImportDecl, IrModule};

use crate::error::CompileError;

pub(crate) fn cranelift_sig_for_builtin(
    builtin: BuiltinId,
    pointer_type: types::Type,
    call_conv: CallConv,
) -> Signature {
    crate::generated_builtin_abi::cranelift_sig_for_builtin_inner(builtin, pointer_type, call_conv)
}

pub(crate) fn get_function_pointer(builtin: BuiltinId) -> *const u8 {
    crate::generated_builtin_abi::get_function_pointer_inner(builtin)
}

pub(crate) fn resolve_import(
    decl: &ImportDecl,
    mode: FloatMode,
) -> Result<BuiltinId, CompileError> {
    match (decl.module_name.as_str(), mode) {
        ("glsl", FloatMode::Q32) => {
            let ac = decl.param_types.len();
            glsl_q32_math_builtin_id(decl.func_name.as_str(), ac).ok_or_else(|| {
                CompileError::unsupported(format!(
                    "unsupported glsl import `{}` (arity {ac})",
                    decl.func_name
                ))
            })
        }
        ("lpir", FloatMode::Q32) => {
            let ac = decl.param_types.len();
            lpir_q32_builtin_id(decl.func_name.as_str(), ac).ok_or_else(|| {
                CompileError::unsupported(format!(
                    "unsupported lpir import `{}` (arity {ac})",
                    decl.func_name
                ))
            })
        }
        ("lpfx", FloatMode::Q32) => {
            let base = lpfx_strip_suffix(&decl.func_name)?;
            let kinds = lpfx_glsl_kinds_from_decl(decl)?;
            glsl_lpfx_q32_builtin_id(base, &kinds).ok_or_else(|| {
                CompileError::unsupported(format!(
                    "unsupported lpfx import `{}` with kinds {:?}",
                    decl.func_name, kinds
                ))
            })
        }
        ("glsl" | "lpir" | "lpfx", FloatMode::F32) => Err(CompileError::unsupported(format!(
            "import `{}::{}` requires FloatMode::Q32",
            decl.module_name, decl.func_name
        ))),
        (m, _) => Err(CompileError::unsupported(format!(
            "unsupported import module `{m}`"
        ))),
    }
}

pub(crate) struct LpirBuiltinFuncIds {
    pub fadd: FuncId,
    pub fsub: FuncId,
    pub fmul: FuncId,
    pub fdiv: FuncId,
    pub fsqrt: FuncId,
    pub fnearest: FuncId,
}

pub(crate) fn declare_module_imports(
    module: &mut impl Module,
    ir: &IrModule,
    pointer_type: types::Type,
) -> Result<Vec<FuncId>, CompileError> {
    let call_conv = module.isa().default_call_conv();
    let mut out = Vec::with_capacity(ir.imports.len());
    for decl in &ir.imports {
        let bid = resolve_import(decl, FloatMode::Q32)?;
        let sig = cranelift_sig_for_builtin(bid, pointer_type, call_conv);
        let id = module
            .declare_function(bid.name(), Linkage::Import, &sig)
            .map_err(|e| CompileError::cranelift(format!("declare import {}: {e}", bid.name())))?;
        out.push(id);
    }
    Ok(out)
}

pub(crate) fn declare_lpir_opcode_builtins(
    module: &mut impl Module,
    pointer_type: types::Type,
) -> Result<LpirBuiltinFuncIds, CompileError> {
    let call_conv = module.isa().default_call_conv();
    let mut declare = |bid: BuiltinId| -> Result<FuncId, CompileError> {
        let sig = cranelift_sig_for_builtin(bid, pointer_type, call_conv);
        module
            .declare_function(bid.name(), Linkage::Import, &sig)
            .map_err(|e| {
                CompileError::cranelift(format!("declare LPIR opcode builtin {}: {e}", bid.name()))
            })
    };
    Ok(LpirBuiltinFuncIds {
        fadd: declare(BuiltinId::LpLpirFaddQ32)?,
        fsub: declare(BuiltinId::LpLpirFsubQ32)?,
        fmul: declare(BuiltinId::LpLpirFmulQ32)?,
        fdiv: declare(BuiltinId::LpLpirFdivQ32)?,
        fsqrt: declare(BuiltinId::LpLpirFsqrtQ32)?,
        fnearest: declare(BuiltinId::LpLpirFnearestQ32)?,
    })
}

pub(crate) fn symbol_lookup_fn() -> Box<dyn Fn(&str) -> Option<*const u8> + Send> {
    Box::new(|name: &str| {
        for builtin in BuiltinId::all() {
            if builtin.name() == name {
                return Some(get_function_pointer(*builtin));
            }
        }
        None
    })
}

fn ir_params_to_glsl_kinds(params: &[lpir::types::IrType]) -> Vec<GlslParamKind> {
    params
        .iter()
        .map(|t| match t {
            lpir::types::IrType::F32 => GlslParamKind::Float,
            lpir::types::IrType::I32 => GlslParamKind::UInt,
        })
        .collect()
}

fn lpfx_glsl_kinds_from_decl(decl: &ImportDecl) -> Result<Vec<GlslParamKind>, CompileError> {
    if let Some(ref enc) = decl.lpfx_glsl_params {
        parse_lpfx_glsl_params_csv(enc).map_err(CompileError::unsupported)
    } else {
        Ok(ir_params_to_glsl_kinds(&decl.param_types))
    }
}

fn parse_lpfx_glsl_params_csv(enc: &str) -> Result<Vec<GlslParamKind>, String> {
    if enc.is_empty() {
        return Ok(Vec::new());
    }
    enc.split(',')
        .map(|t| match t.trim() {
            "Float" => Ok(GlslParamKind::Float),
            "Int" => Ok(GlslParamKind::Int),
            "UInt" => Ok(GlslParamKind::UInt),
            "Vec2" => Ok(GlslParamKind::Vec2),
            "Vec3" => Ok(GlslParamKind::Vec3),
            "Vec4" => Ok(GlslParamKind::Vec4),
            "IVec2" => Ok(GlslParamKind::IVec2),
            "IVec3" => Ok(GlslParamKind::IVec3),
            "IVec4" => Ok(GlslParamKind::IVec4),
            "UVec2" => Ok(GlslParamKind::UVec2),
            "UVec3" => Ok(GlslParamKind::UVec3),
            "UVec4" => Ok(GlslParamKind::UVec4),
            "BVec2" => Ok(GlslParamKind::BVec2),
            "BVec3" => Ok(GlslParamKind::BVec3),
            "BVec4" => Ok(GlslParamKind::BVec4),
            other => Err(format!("unknown LPFX glsl param tag `{other}`")),
        })
        .collect()
}

fn lpfx_strip_suffix(func_name: &str) -> Result<&str, CompileError> {
    let (base, tail) = func_name.rsplit_once('_').ok_or_else(|| {
        CompileError::unsupported(format!("malformed lpfx import name `{func_name}`"))
    })?;
    tail.parse::<u32>().map_err(|_| {
        CompileError::unsupported(format!("malformed lpfx import name `{func_name}`"))
    })?;
    Ok(base)
}
