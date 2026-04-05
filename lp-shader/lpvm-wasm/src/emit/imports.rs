//! Prune unused LPIR imports and map declarations to `builtins` WASM names.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::FloatMode;
use lpir::{CalleeRef, ImportDecl, IrFunction, IrModule, IrType, Op};
use lps_builtin_ids::{
    BuiltinId, GlslParamKind, glsl_lpfx_q32_builtin_id, glsl_q32_math_builtin_id,
    lpir_q32_builtin_id, vm_q32_builtin_id,
};

/// After pruning: WASM import function index `i` corresponds to `filtered[i]`.
pub(crate) struct FilteredImports {
    pub decls: Vec<ImportDecl>,
    /// `remap[old_index] = Some(wasm_import_func_index)` for kept imports, else `None`.
    pub remap: Vec<Option<u32>>,
    pub full_count: u32,
}

fn collect_used_import_indices(ir: &IrModule) -> BTreeSet<u32> {
    let n = ir.imports.len() as u32;
    let mut used = BTreeSet::new();
    let mut needs_lpir_sqrt = false;
    let mut needs_glsl_round = false;

    for f in &ir.functions {
        for op in &f.body {
            if let Op::Call { callee, .. } = op {
                if callee.0 < n {
                    used.insert(callee.0);
                }
            }
            // Track LPIR ops that resolve to builtin calls at emit time (Q32 mode)
            if matches!(op, Op::Fsqrt { .. }) {
                needs_lpir_sqrt = true;
            }
            if matches!(op, Op::Fnearest { .. }) {
                needs_glsl_round = true;
            }
        }
    }

    // Add imports for LPIR ops that call builtins in Q32 mode
    if needs_lpir_sqrt {
        if let Some(idx) = find_import_index(ir, "lpir", "sqrt") {
            used.insert(idx);
        }
    }
    if needs_glsl_round {
        if let Some(idx) = find_import_index(ir, "glsl", "round") {
            used.insert(idx);
        }
    }

    used
}

fn find_import_index(ir: &IrModule, module: &str, func_name: &str) -> Option<u32> {
    ir.imports
        .iter()
        .enumerate()
        .find(|(_, d)| d.module_name == module && d.func_name == func_name)
        .map(|(i, _)| i as u32)
}

fn ir_params_to_glsl_kinds(params: &[IrType]) -> Vec<GlslParamKind> {
    params
        .iter()
        .map(|t| match t {
            IrType::F32 => GlslParamKind::Float,
            IrType::I32 | IrType::Pointer => GlslParamKind::UInt,
        })
        .collect()
}

fn lpfx_glsl_kinds_from_decl(decl: &ImportDecl) -> Result<Vec<GlslParamKind>, String> {
    if let Some(ref enc) = decl.lpfx_glsl_params {
        parse_lpfx_glsl_params_csv(enc)
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

fn resolve_builtin_id(decl: &ImportDecl) -> Result<BuiltinId, String> {
    match decl.module_name.as_str() {
        "glsl" => {
            let ac = decl.param_types.len();
            glsl_q32_math_builtin_id(decl.func_name.as_str(), ac).ok_or_else(|| {
                format!(
                    "unsupported glsl import `{}` (arg count {ac})",
                    decl.func_name
                )
            })
        }
        "lpir" => {
            let ac = decl.param_types.len();
            lpir_q32_builtin_id(decl.func_name.as_str(), ac).ok_or_else(|| {
                format!(
                    "unsupported lpir import `{}` (arg count {ac})",
                    decl.func_name
                )
            })
        }
        "lpfx" => {
            let base = lpfx_strip_suffix(&decl.func_name)?;
            let kinds = lpfx_glsl_kinds_from_decl(decl)?;
            glsl_lpfx_q32_builtin_id(base, &kinds).ok_or_else(|| {
                format!(
                    "unsupported lpfx import `{}` with {:?}",
                    decl.func_name, kinds
                )
            })
        }
        "vm" => {
            let ac = decl.param_types.len();
            vm_q32_builtin_id(decl.func_name.as_str(), ac).ok_or_else(|| {
                format!(
                    "unsupported vm import `{}` (arg count {ac})",
                    decl.func_name
                )
            })
        }
        m => Err(format!("unsupported import module `{m}`")),
    }
}

/// `lpfx_saturate_3` → `lpfx_saturate`.
fn lpfx_strip_suffix(func_name: &str) -> Result<&str, String> {
    let (base, tail) = func_name
        .rsplit_once('_')
        .ok_or_else(|| format!("malformed lpfx import name `{func_name}`"))?;
    tail.parse::<u32>()
        .map_err(|_| format!("malformed lpfx import name `{func_name}`"))?;
    Ok(base)
}

pub(crate) fn build_filtered_imports(ir: &IrModule) -> Result<FilteredImports, String> {
    let used = collect_used_import_indices(ir);
    let full_count = ir.imports.len() as u32;
    let mut remap = vec![None; ir.imports.len()];
    let mut decls = Vec::new();
    let mut next_wasm = 0u32;
    for (i, decl) in ir.imports.iter().enumerate() {
        if !used.contains(&(i as u32)) {
            continue;
        }
        let _ = resolve_builtin_id(decl)?;
        remap[i] = Some(next_wasm);
        decls.push(decl.clone());
        next_wasm += 1;
    }
    Ok(FilteredImports {
        decls,
        remap,
        full_count,
    })
}

/// True if any user function calls an import that uses the result-pointer WASM ABI
/// (non-scalar return via hidden pointer; callee has zero WASM results).
pub(crate) fn module_needs_result_ptr_calls(ir: &IrModule) -> bool {
    let n = ir.imports.len() as u32;
    for f in &ir.functions {
        for op in &f.body {
            if let Op::Call { callee, .. } = op {
                if callee.0 < n && import_uses_result_pointer_abi(ir, callee.0 as usize) {
                    return true;
                }
            }
        }
    }
    false
}

/// Whether `ir.imports[import_idx]` uses result-pointer calling convention in WASM.
pub(crate) fn import_uses_result_pointer_abi(ir: &IrModule, import_idx: usize) -> bool {
    let decl = match ir.imports.get(import_idx) {
        Some(d) => d,
        None => return false,
    };
    if decl.return_types.is_empty() {
        return false;
    }
    let Ok(bid) = resolve_builtin_id(decl) else {
        return false;
    };
    let (_params, wasm_results) = super::builtin_wasm_import_types::wasm_import_val_types(bid);
    wasm_results.is_empty()
}

/// Max byte size of temporary result buffer needed for result-pointer builtin calls in `f`.
pub(crate) fn max_result_ptr_buffer_bytes(ir: &IrModule, f: &IrFunction) -> u32 {
    let n = ir.imports.len() as u32;
    let mut max_b = 0u32;
    for op in &f.body {
        if let Op::Call {
            callee, results, ..
        } = op
        {
            if callee.0 < n && import_uses_result_pointer_abi(ir, callee.0 as usize) {
                let count = f.pool_slice(*results).len() as u32;
                max_b = max_b.max(count.saturating_mul(4));
            }
        }
    }
    max_b
}

pub(crate) fn import_decl_val_types(
    decl: &ImportDecl,
    _mode: FloatMode,
) -> Result<(Vec<wasm_encoder::ValType>, Vec<wasm_encoder::ValType>), String> {
    let bid = resolve_builtin_id(decl)?;
    Ok(super::builtin_wasm_import_types::wasm_import_val_types(bid))
}

pub(crate) fn builtins_wasm_name(decl: &ImportDecl) -> Result<&'static str, String> {
    Ok(resolve_builtin_id(decl)?.name())
}

pub(crate) fn import_callee(
    ir: &IrModule,
    module: &str,
    func_name: &str,
) -> Result<CalleeRef, String> {
    ir.imports
        .iter()
        .enumerate()
        .find(|(_, d)| d.module_name == module && d.func_name == func_name)
        .map(|(i, _)| CalleeRef(i as u32))
        .ok_or_else(|| format!("missing import @{module}::{func_name}"))
}
