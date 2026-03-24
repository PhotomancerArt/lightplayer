//! LPFX builtin calls → `@lpfx::…` imports, scalar and vector value arguments, and out-parameters.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::{CalleeRef, ImportDecl, IrType, ModuleBuilder, Op, VReg};
use naga::{
    AddressSpace, Block, Expression, Function, Handle, LocalVariable, Module, ScalarKind,
    Statement, TypeInner, VectorSize,
};

use crate::NagaModule;
use crate::lower_ctx::{VRegVec, naga_scalar_to_ir_type, naga_type_to_ir_types, vector_size_usize};
use crate::lower_error::LowerError;

/// How one LPFX callee argument maps to LPIR call operands (one entry per Naga parameter).
#[derive(Clone, Debug)]
pub(crate) enum LpfxArgKind {
    /// Scalar value (`F32` / `I32`).
    Value,
    /// Vector value: `count` scalar params after lowering.
    ValueVector(u8),
    /// Out-pointer: pass slot address (`I32`); callee writes one scalar.
    OutScalar(IrType),
    /// Out-pointer to vector: one `I32` slot address; slot holds `count` scalars.
    OutVector(IrType, u8),
}

pub(crate) fn register_lpfx_imports(
    mb: &mut ModuleBuilder,
    naga_module: &NagaModule,
) -> Result<BTreeMap<Handle<Function>, CalleeRef>, LowerError> {
    let handles = collect_lpfx_callee_handles(naga_module);
    let mut map = BTreeMap::new();
    for h in handles {
        let decl = build_lpfx_import_decl(&naga_module.module, h)?;
        let r = mb.add_import(decl);
        map.insert(h, r);
    }
    Ok(map)
}

fn collect_lpfx_callee_handles(naga_module: &NagaModule) -> Vec<Handle<Function>> {
    let mut seen = BTreeSet::new();
    for (fh, _) in &naga_module.functions {
        let f = &naga_module.module.functions[*fh];
        walk_block_for_lpfx_calls(&naga_module.module, f, &f.body, &mut seen);
    }
    seen.into_iter().collect()
}

fn walk_block_for_lpfx_calls(
    module: &Module,
    func: &Function,
    block: &Block,
    seen: &mut BTreeSet<Handle<Function>>,
) {
    for stmt in block.iter() {
        match stmt {
            Statement::Call {
                function: callee, ..
            } => {
                if let Some(name) = module.functions[*callee].name.as_deref() {
                    if name.starts_with("lpfx_") {
                        seen.insert(*callee);
                    }
                }
            }
            Statement::Block(inner) => walk_block_for_lpfx_calls(module, func, inner, seen),
            Statement::If { accept, reject, .. } => {
                walk_block_for_lpfx_calls(module, func, accept, seen);
                walk_block_for_lpfx_calls(module, func, reject, seen);
            }
            Statement::Switch { cases, .. } => {
                for c in cases {
                    walk_block_for_lpfx_calls(module, func, &c.body, seen);
                }
            }
            Statement::Loop {
                body, continuing, ..
            } => {
                walk_block_for_lpfx_calls(module, func, body, seen);
                walk_block_for_lpfx_calls(module, func, continuing, seen);
            }
            _ => {}
        }
    }
}

fn lpfx_glsl_params_csv(module: &Module, callee: Handle<Function>) -> Result<String, LowerError> {
    let f = &module.functions[callee];
    let mut out = Vec::new();
    for arg in &f.arguments {
        out.push(lpfx_glsl_param_token(module, arg.ty)?);
    }
    Ok(out.join(","))
}

fn lpfx_glsl_param_token(module: &Module, ty: Handle<naga::Type>) -> Result<String, LowerError> {
    match &module.types[ty].inner {
        TypeInner::Pointer { base, .. } => lpfx_pointee_token(module, *base),
        TypeInner::Scalar(scalar) => match scalar.kind {
            ScalarKind::Float => Ok(String::from("Float")),
            ScalarKind::Sint => Ok(String::from("Int")),
            ScalarKind::Uint => Ok(String::from("UInt")),
            ScalarKind::Bool | ScalarKind::AbstractInt | ScalarKind::AbstractFloat => Err(
                LowerError::UnsupportedType(String::from("LPFX scalar parameter kind")),
            ),
        },
        TypeInner::Vector { size, scalar, .. } => lpfx_vector_token(*size, scalar.kind),
        _ => Err(LowerError::UnsupportedType(String::from(
            "LPFX parameter type for glsl tag",
        ))),
    }
}

fn lpfx_pointee_token(module: &Module, base_ty: Handle<naga::Type>) -> Result<String, LowerError> {
    match &module.types[base_ty].inner {
        TypeInner::Scalar(scalar) => match scalar.kind {
            ScalarKind::Float => Ok(String::from("Float")),
            ScalarKind::Sint => Ok(String::from("Int")),
            ScalarKind::Uint => Ok(String::from("UInt")),
            ScalarKind::Bool | ScalarKind::AbstractInt | ScalarKind::AbstractFloat => Err(
                LowerError::UnsupportedType(String::from("LPFX out scalar kind")),
            ),
        },
        TypeInner::Vector { size, scalar, .. } => lpfx_vector_token(*size, scalar.kind),
        _ => Err(LowerError::UnsupportedType(String::from(
            "LPFX out pointee",
        ))),
    }
}

fn lpfx_vector_token(size: VectorSize, kind: ScalarKind) -> Result<String, LowerError> {
    Ok(match (size, kind) {
        (VectorSize::Bi, ScalarKind::Float) => String::from("Vec2"),
        (VectorSize::Tri, ScalarKind::Float) => String::from("Vec3"),
        (VectorSize::Quad, ScalarKind::Float) => String::from("Vec4"),
        (VectorSize::Bi, ScalarKind::Uint) => String::from("UVec2"),
        (VectorSize::Tri, ScalarKind::Uint) => String::from("UVec3"),
        (VectorSize::Quad, ScalarKind::Uint) => String::from("UVec4"),
        _ => {
            return Err(LowerError::UnsupportedType(String::from(
                "LPFX vector parameter kind",
            )));
        }
    })
}

fn build_lpfx_import_decl(
    module: &Module,
    callee: Handle<Function>,
) -> Result<ImportDecl, LowerError> {
    let f = &module.functions[callee];
    let name = f
        .name
        .clone()
        .ok_or_else(|| LowerError::Internal(String::from("LPFX callee missing name")))?;
    let lpfx_glsl_params = Some(lpfx_glsl_params_csv(module, callee)?);
    let mut param_types = Vec::new();
    for arg in &f.arguments {
        match &module.types[arg.ty].inner {
            TypeInner::Pointer {
                base,
                space: AddressSpace::Function,
            } => match &module.types[*base].inner {
                TypeInner::Scalar(scalar) => {
                    param_types.push(IrType::I32);
                    let _ = naga_scalar_to_ir_type(scalar.kind)?;
                }
                TypeInner::Vector { .. } => {
                    param_types.push(IrType::I32);
                }
                _ => {
                    return Err(LowerError::UnsupportedType(format!(
                        "LPFX out-pointer base {:?}",
                        module.types[*base].inner
                    )));
                }
            },
            TypeInner::Scalar(scalar) => {
                param_types.push(naga_scalar_to_ir_type(scalar.kind)?);
            }
            TypeInner::Vector { size, scalar, .. } => {
                let ir_ty = naga_scalar_to_ir_type(scalar.kind)?;
                for _ in 0..vector_size_usize(*size) {
                    param_types.push(ir_ty);
                }
            }
            _ => {
                return Err(LowerError::UnsupportedType(format!(
                    "LPFX argument type {:?}",
                    module.types[arg.ty].inner
                )));
            }
        }
    }
    let return_types = if let Some(res) = &f.result {
        let inner = &module.types[res.ty].inner;
        let tys = naga_type_to_ir_types(inner)?;
        tys.to_vec()
    } else {
        Vec::new()
    };
    let func_name = format!("{name}_{}", callee.index());
    Ok(ImportDecl {
        module_name: String::from("lpfx"),
        func_name,
        param_types,
        return_types,
        lpfx_glsl_params,
    })
}

fn lpfx_arg_kinds(
    module: &Module,
    callee: Handle<Function>,
) -> Result<Vec<LpfxArgKind>, LowerError> {
    let f = &module.functions[callee];
    let mut out = Vec::new();
    for arg in &f.arguments {
        match &module.types[arg.ty].inner {
            TypeInner::Pointer {
                base,
                space: AddressSpace::Function,
            } => match &module.types[*base].inner {
                TypeInner::Scalar(scalar) => {
                    out.push(LpfxArgKind::OutScalar(naga_scalar_to_ir_type(scalar.kind)?));
                }
                TypeInner::Vector { size, scalar, .. } => {
                    out.push(LpfxArgKind::OutVector(
                        naga_scalar_to_ir_type(scalar.kind)?,
                        vector_size_usize(*size) as u8,
                    ));
                }
                _ => {
                    return Err(LowerError::UnsupportedType(String::from(
                        "LPFX out-pointer base",
                    )));
                }
            },
            TypeInner::Scalar(scalar) => {
                let _ = naga_scalar_to_ir_type(scalar.kind)?;
                out.push(LpfxArgKind::Value);
            }
            TypeInner::Vector { size, scalar, .. } => {
                let _ = naga_scalar_to_ir_type(scalar.kind)?;
                out.push(LpfxArgKind::ValueVector(vector_size_usize(*size) as u8));
            }
            _ => {
                return Err(LowerError::UnsupportedType(String::from(
                    "LPFX unsupported value arg",
                )));
            }
        }
    }
    Ok(out)
}

fn out_pointer_local(
    func: &Function,
    expr: Handle<Expression>,
) -> Result<Handle<LocalVariable>, LowerError> {
    match &func.expressions[expr] {
        Expression::LocalVariable(lv) => Ok(*lv),
        _ => Err(LowerError::UnsupportedExpression(String::from(
            "LPFX out argument must be a local variable",
        ))),
    }
}

/// Lower a call to an LPFX import (empty-bodied Naga stub).
pub(crate) fn lower_lpfx_call(
    ctx: &mut crate::lower_ctx::LowerCtx<'_>,
    callee: Handle<Function>,
    arguments: &[Handle<Expression>],
    result: Option<Handle<Expression>>,
) -> Result<(), LowerError> {
    let callee_ref = *ctx.lpfx_map.get(&callee).ok_or_else(|| {
        LowerError::Internal(format!(
            "LPFX callee {:?} not registered",
            ctx.module.functions[callee].name
        ))
    })?;
    let kinds = lpfx_arg_kinds(ctx.module, callee)?;
    if kinds.len() != arguments.len() {
        return Err(LowerError::Internal(String::from(
            "LPFX arg count mismatch",
        )));
    }
    let f = &ctx.module.functions[callee];
    let mut arg_vs: Vec<VReg> = Vec::new();
    let mut outs: Vec<(VReg, VReg, IrType)> = Vec::new();
    let mut vec_outs: Vec<(VReg, VRegVec, IrType, usize)> = Vec::new();

    for (kind, &arg_expr) in kinds.iter().zip(arguments.iter()) {
        match kind {
            LpfxArgKind::Value => {
                arg_vs.push(ctx.ensure_expr(arg_expr)?);
            }
            LpfxArgKind::ValueVector(n) => {
                let vs = ctx.ensure_expr_vec(arg_expr)?;
                if vs.len() != *n as usize {
                    return Err(LowerError::Internal(format!(
                        "LPFX ValueVector expected {n} components, got {}",
                        vs.len()
                    )));
                }
                for v in &vs {
                    arg_vs.push(*v);
                }
            }
            LpfxArgKind::OutScalar(ir) => {
                let lv = out_pointer_local(ctx.func, arg_expr)?;
                let dsts = ctx.resolve_local(lv)?;
                if dsts.len() != 1 {
                    return Err(LowerError::Internal(String::from(
                        "LPFX OutScalar local width",
                    )));
                }
                let dst = dsts[0];
                let slot = ctx.fb.alloc_slot(4);
                let addr = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::SlotAddr { dst: addr, slot });
                arg_vs.push(addr);
                outs.push((addr, dst, *ir));
            }
            LpfxArgKind::OutVector(ir_ty, count) => {
                let lv = out_pointer_local(ctx.func, arg_expr)?;
                let dsts = ctx.resolve_local(lv)?;
                let n = *count as usize;
                if dsts.len() != n {
                    return Err(LowerError::Internal(format!(
                        "LPFX OutVector local width {} vs {n}",
                        dsts.len()
                    )));
                }
                let slot = ctx.fb.alloc_slot(n as u32 * 4);
                let addr = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::SlotAddr { dst: addr, slot });
                arg_vs.push(addr);
                vec_outs.push((addr, dsts, *ir_ty, n));
            }
        }
    }

    let mut result_vs: Vec<VReg> = Vec::new();
    if let Some(res_h) = result {
        let res_ty = f
            .result
            .as_ref()
            .ok_or_else(|| LowerError::Internal(String::from("LPFX void with result expr")))?;
        let inner = &ctx.module.types[res_ty.ty].inner;
        let ir_tys = naga_type_to_ir_types(inner)?;
        let mut vregs = VRegVec::new();
        for ty in &ir_tys {
            let v = ctx.fb.alloc_vreg(*ty);
            vregs.push(v);
            result_vs.push(v);
        }
        if let Some(slot) = ctx.expr_cache.get_mut(res_h.index()) {
            *slot = Some(vregs);
        }
    } else if f.result.is_some() {
        return Err(LowerError::Internal(String::from(
            "LPFX return missing result expression",
        )));
    }

    ctx.fb.push_call(callee_ref, &arg_vs, &result_vs);

    for (addr, dst, ir_ty) in outs {
        let tmp = ctx.fb.alloc_vreg(ir_ty);
        ctx.fb.push(Op::Load {
            dst: tmp,
            base: addr,
            offset: 0,
        });
        ctx.fb.push(Op::Copy { dst, src: tmp });
    }

    for (addr, dsts, ir_ty, n) in vec_outs {
        for i in 0..n {
            let tmp = ctx.fb.alloc_vreg(ir_ty);
            ctx.fb.push(Op::Load {
                dst: tmp,
                base: addr,
                offset: (i as u32) * 4,
            });
            ctx.fb.push(Op::Copy {
                dst: dsts[i],
                src: tmp,
            });
        }
    }

    Ok(())
}
