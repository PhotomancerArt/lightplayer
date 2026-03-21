//! LPFX calls: resolve `lpfx_*` Naga prototypes to `builtins` WASM imports (Q32).

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_glsl_builtin_ids::{BuiltinId, GlslParamKind, glsl_lpfx_q32_builtin_id};
use lp_glsl_naga::FloatMode;
use naga::{Block, Function, Handle, Module, ScalarKind, Statement, TypeInner, VectorSize};
use wasm_encoder::{Instruction, MemArg, ValType};

use crate::emit::emit_expr;
use crate::locals::LocalAlloc;
use crate::types::type_handle_component_count;

fn pi32(n: usize) -> Vec<ValType> {
    (0..n).map(|_| ValType::I32).collect()
}

/// WASM import signature for Q32 LPFX (`extern "C"` / lp-glsl-builtins), or `None` if not LPFX Q32.
pub(crate) fn q32_lpfx_wasm_signature(id: BuiltinId) -> Option<(Vec<ValType>, Vec<ValType>)> {
    use BuiltinId::*;
    match id {
        LpfxHash1 => Some((pi32(2), pi32(1))),
        LpfxHash2 => Some((pi32(3), pi32(1))),
        LpfxHash3 => Some((pi32(4), pi32(1))),
        LpfxSaturateQ32 => Some((pi32(1), pi32(1))),
        LpfxSaturateVec3Q32 => Some((pi32(4), Vec::new())),
        LpfxSaturateVec4Q32 => Some((pi32(5), Vec::new())),
        LpfxHue2rgbQ32 => Some((pi32(2), Vec::new())),
        LpfxHsv2rgbQ32 => Some((pi32(4), Vec::new())),
        LpfxHsv2rgbVec4Q32 => Some((pi32(5), Vec::new())),
        LpfxRgb2hsvQ32 => Some((pi32(4), Vec::new())),
        LpfxRgb2hsvVec4Q32 => Some((pi32(5), Vec::new())),
        LpfxFbm2Q32 => Some((pi32(4), pi32(1))),
        LpfxFbm3Q32 => Some((pi32(4), pi32(1))),
        LpfxFbm3TileQ32 => Some((pi32(6), pi32(1))),
        LpfxGnoise1Q32 => Some((pi32(2), pi32(1))),
        LpfxGnoise2Q32 => Some((pi32(3), pi32(1))),
        LpfxGnoise3Q32 => Some((pi32(4), pi32(1))),
        LpfxGnoise3TileQ32 => Some((pi32(5), pi32(1))),
        LpfxRandom1Q32 => Some((pi32(2), pi32(1))),
        LpfxRandom2Q32 => Some((pi32(3), pi32(1))),
        LpfxRandom3Q32 => Some((pi32(4), pi32(1))),
        LpfxSnoise1Q32 => Some((pi32(2), pi32(1))),
        LpfxSnoise2Q32 => Some((pi32(3), pi32(1))),
        LpfxSnoise3Q32 => Some((pi32(4), pi32(1))),
        LpfxSrandom1Q32 => Some((pi32(2), pi32(1))),
        LpfxSrandom2Q32 => Some((pi32(3), pi32(1))),
        LpfxSrandom3Q32 => Some((pi32(4), pi32(1))),
        LpfxSrandom3TileQ32 => Some((pi32(6), Vec::new())),
        LpfxSrandom3VecQ32 => Some((pi32(5), Vec::new())),
        LpfxWorley2Q32 => Some((pi32(3), pi32(1))),
        LpfxWorley3Q32 => Some((pi32(4), pi32(1))),
        LpfxWorley2ValueQ32 => Some((pi32(3), pi32(1))),
        LpfxWorley3ValueQ32 => Some((pi32(4), pi32(1))),
        LpfxPsrdnoise2Q32 => Some((pi32(7), pi32(1))),
        LpfxPsrdnoise3Q32 => Some((pi32(9), pi32(1))),
        _ => None,
    }
}

fn naga_ty_to_glsl_param_kind(
    module: &Module,
    ty: Handle<naga::Type>,
) -> Result<GlslParamKind, String> {
    match &module.types[ty].inner {
        TypeInner::Pointer { base, .. } => naga_ty_to_glsl_param_kind(module, *base),
        TypeInner::Scalar(s) if s.width == 4 => match s.kind {
            ScalarKind::Float => Ok(GlslParamKind::Float),
            ScalarKind::Sint => Ok(GlslParamKind::Int),
            ScalarKind::Uint => Ok(GlslParamKind::UInt),
            ScalarKind::Bool => Ok(GlslParamKind::Bool),
            _ => Err(format!("lpfx: unsupported scalar {:?}", s)),
        },
        TypeInner::Vector { size, scalar } if scalar.width == 4 => match scalar.kind {
            ScalarKind::Float => match size {
                VectorSize::Bi => Ok(GlslParamKind::Vec2),
                VectorSize::Tri => Ok(GlslParamKind::Vec3),
                VectorSize::Quad => Ok(GlslParamKind::Vec4),
            },
            ScalarKind::Sint => match size {
                VectorSize::Bi => Ok(GlslParamKind::IVec2),
                VectorSize::Tri => Ok(GlslParamKind::IVec3),
                VectorSize::Quad => Ok(GlslParamKind::IVec4),
            },
            ScalarKind::Uint => match size {
                VectorSize::Bi => Ok(GlslParamKind::UVec2),
                VectorSize::Tri => Ok(GlslParamKind::UVec3),
                VectorSize::Quad => Ok(GlslParamKind::UVec4),
            },
            ScalarKind::Bool => match size {
                VectorSize::Bi => Ok(GlslParamKind::BVec2),
                VectorSize::Tri => Ok(GlslParamKind::BVec3),
                VectorSize::Quad => Ok(GlslParamKind::BVec4),
            },
            _ => Err(format!("lpfx: unsupported vector scalar {:?}", scalar)),
        },
        other => Err(format!("lpfx: unsupported param type {other:?}")),
    }
}

pub(crate) fn resolve_lpfx_q32_builtin(
    module: &Module,
    callee: Handle<Function>,
) -> Option<BuiltinId> {
    let f = &module.functions[callee];
    let name = f.name.as_deref()?;
    if !name.starts_with("lpfx_") {
        return None;
    }
    let mut kinds = Vec::with_capacity(f.arguments.len());
    for arg in &f.arguments {
        kinds.push(naga_ty_to_glsl_param_kind(module, arg.ty).ok()?);
    }
    glsl_lpfx_q32_builtin_id(name, &kinds)
}

fn collect_lpfx_in_block(module: &Module, block: &Block, out: &mut BTreeSet<BuiltinId>) {
    for stmt in block.iter() {
        match stmt {
            Statement::Call {
                function,
                arguments: _,
                result: _,
            } => {
                if let Some(id) = resolve_lpfx_q32_builtin(module, *function) {
                    out.insert(id);
                }
            }
            Statement::Block(b) => collect_lpfx_in_block(module, b, out),
            Statement::If { accept, reject, .. } => {
                collect_lpfx_in_block(module, accept, out);
                collect_lpfx_in_block(module, reject, out);
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                collect_lpfx_in_block(module, body, out);
                collect_lpfx_in_block(module, continuing, out);
            }
            Statement::Switch { cases, .. } => {
                for c in cases {
                    collect_lpfx_in_block(module, &c.body, out);
                }
            }
            _ => {}
        }
    }
}

pub(crate) fn collect_lpfx_builtin_ids(
    module: &Module,
    user_funcs: &[(Handle<Function>, lp_glsl_naga::FunctionInfo)],
) -> BTreeSet<BuiltinId> {
    let mut set = BTreeSet::new();
    for (fh, _) in user_funcs {
        let f = &module.functions[*fh];
        collect_lpfx_in_block(module, &f.body, &mut set);
    }
    set
}

fn is_pointer_ty(module: &Module, ty: Handle<naga::Type>) -> bool {
    matches!(&module.types[ty].inner, TypeInner::Pointer { .. })
}

fn pointer_inner_ty(module: &Module, ty: Handle<naga::Type>) -> Handle<naga::Type> {
    match &module.types[ty].inner {
        TypeInner::Pointer { base, .. } => *base,
        _ => ty,
    }
}

fn mem_load_i32(wasm_fn: &mut wasm_encoder::Function, offset: u32) {
    wasm_fn.instruction(&Instruction::I32Const(0));
    wasm_fn.instruction(&Instruction::I32Load(MemArg {
        offset: u64::from(offset),
        align: 2,
        memory_index: 0,
    }));
}

pub(crate) fn emit_lpfx_import_call(
    module: &Module,
    func: &Function,
    callee: Handle<Function>,
    arguments: &[Handle<naga::Expression>],
    result: Option<Handle<naga::Expression>>,
    wasm_fn: &mut wasm_encoder::Function,
    mode: FloatMode,
    alloc: &LocalAlloc,
    import_func_index: u32,
    bid: BuiltinId,
    scratch_cursor: &core::cell::Cell<u32>,
) -> Result<(), String> {
    if !matches!(mode, FloatMode::Q32) {
        return Err(String::from("WASM codegen: LPFX imports require Q32 mode"));
    }
    let callee_fn = &module.functions[callee];
    let (wasm_params, wasm_results) = q32_lpfx_wasm_signature(bid)
        .ok_or_else(|| format!("WASM codegen: missing wasm signature for {bid:?}"))?;

    let prepend_result_ptr = wasm_results.is_empty()
        && !callee_fn
            .arguments
            .iter()
            .any(|a| is_pointer_ty(module, a.ty))
        && matches!(
            bid,
            BuiltinId::LpfxSaturateVec3Q32
                | BuiltinId::LpfxSaturateVec4Q32
                | BuiltinId::LpfxHue2rgbQ32
                | BuiltinId::LpfxHsv2rgbQ32
                | BuiltinId::LpfxHsv2rgbVec4Q32
                | BuiltinId::LpfxRgb2hsvQ32
                | BuiltinId::LpfxRgb2hsvVec4Q32
                | BuiltinId::LpfxSrandom3TileQ32
                | BuiltinId::LpfxSrandom3VecQ32
        );

    let mut flat_slots: usize = 0;
    if prepend_result_ptr {
        flat_slots += 1;
    }
    for arg_decl in &callee_fn.arguments {
        if is_pointer_ty(module, arg_decl.ty) {
            flat_slots += 1;
        } else {
            flat_slots += type_handle_component_count(module, arg_decl.ty) as usize;
        }
    }
    if flat_slots != wasm_params.len() {
        return Err(format!(
            "WASM codegen: lpfx flat arg slots {flat_slots} vs wasm {}",
            wasm_params.len()
        ));
    }
    if callee_fn.arguments.len() != arguments.len() {
        return Err(format!(
            "WASM codegen: lpfx Naga arg count {} vs {}",
            callee_fn.arguments.len(),
            arguments.len()
        ));
    }

    let mut scratch_writes: Vec<(u32, u32)> = Vec::new();

    if prepend_result_ptr {
        let ret_ty = callee_fn
            .result
            .as_ref()
            .ok_or_else(|| {
                String::from("WASM codegen: lpfx implicit ptr needs callee result type")
            })?
            .ty;
        let dim = type_handle_component_count(module, ret_ty);
        let bytes = dim.saturating_mul(4);
        let mut off = scratch_cursor.get();
        let align = 4u32;
        off = (off + align - 1) & !(align - 1);
        scratch_cursor.set(off.saturating_add(bytes));
        scratch_writes.push((off, dim));
        wasm_fn.instruction(&Instruction::I32Const(off as i32));
    }

    for (arg_decl, &arg_h) in callee_fn.arguments.iter().zip(arguments.iter()) {
        if is_pointer_ty(module, arg_decl.ty) {
            let inner = pointer_inner_ty(module, arg_decl.ty);
            let bytes = type_handle_component_count(module, inner).saturating_mul(4);
            let mut off = scratch_cursor.get();
            let align = 4u32;
            off = (off + align - 1) & !(align - 1);
            scratch_cursor.set(off + bytes);
            scratch_writes.push((off, type_handle_component_count(module, inner)));
            wasm_fn.instruction(&Instruction::I32Const(off as i32));
        } else {
            let expected = type_handle_component_count(module, arg_decl.ty);
            let actual = crate::emit_vec::expr_component_count(module, func, arg_h)?;
            emit_expr(module, func, arg_h, wasm_fn, mode, alloc)?;
            if expected == 1 && actual > 1 {
                let base = alloc.alloc_temp_n(actual)?;
                for i in (0..actual).rev() {
                    wasm_fn.instruction(&Instruction::LocalSet(base + i));
                }
                wasm_fn.instruction(&Instruction::LocalGet(base));
            } else if expected > 1 && expected != actual {
                return Err(format!(
                    "WASM codegen: lpfx call arg width {actual} vs expected {expected}"
                ));
            }
        }
    }

    wasm_fn.instruction(&Instruction::Call(import_func_index));

    match (wasm_results.as_slice(), result) {
        ([], None) => Ok(()),
        ([], Some(res_h)) => {
            let (off, dim) = scratch_writes
                .first()
                .copied()
                .ok_or_else(|| String::from("WASM codegen: void lpfx missing out scratch"))?;
            let base = alloc.call_result_wasm_base(res_h).ok_or_else(|| {
                String::from("WASM codegen: lpfx vector result missing CallResult locals")
            })?;
            for i in 0..dim {
                mem_load_i32(wasm_fn, off + i * 4);
                wasm_fn.instruction(&Instruction::LocalSet(base + i));
            }
            Ok(())
        }
        ([ValType::I32], Some(res_h)) => {
            let base = alloc
                .call_result_wasm_base(res_h)
                .ok_or_else(|| String::from("WASM codegen: lpfx scalar result missing locals"))?;
            wasm_fn.instruction(&Instruction::LocalSet(base));
            Ok(())
        }
        ([ValType::I32], None) => {
            wasm_fn.instruction(&Instruction::Drop);
            Ok(())
        }
        _ => Err(format!(
            "WASM codegen: unsupported lpfx result pattern {:?} / {:?}",
            wasm_results, result
        )),
    }
}
