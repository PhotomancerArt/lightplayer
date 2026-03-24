//! LPFX builtin calls → `@lpfx::…` imports and slot-based out-parameters (scalar subset).

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use lpir::{CalleeRef, ImportDecl, IrType, ModuleBuilder, Op, VReg};
use naga::{
    AddressSpace, Block, Expression, Function, Handle, LocalVariable, Module, Statement, TypeInner,
};

use crate::NagaModule;
use crate::lower_ctx::naga_scalar_to_ir_type;
use crate::lower_error::LowerError;

/// How one LPFX callee argument maps to LPIR call operands.
#[derive(Clone, Debug)]
pub(crate) enum LpfxArgKind {
    /// Pass lowered value (`F32` / `I32`).
    Value,
    /// Out-pointer: pass slot address (`I32`); callee writes one scalar to the slot.
    OutScalar(IrType),
}

/// Register `@lpfx` imports for every distinct `lpfx_*` callee used from exported functions.
/// Returns `CalleeRef` per Naga function handle (stable with [`ModuleBuilder`] order).
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

fn build_lpfx_import_decl(
    module: &Module,
    callee: Handle<Function>,
) -> Result<ImportDecl, LowerError> {
    let f = &module.functions[callee];
    let name = f
        .name
        .clone()
        .ok_or_else(|| LowerError::Internal(String::from("LPFX callee missing name")))?;
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
                    return Err(LowerError::UnsupportedType(String::from(
                        "LPFX vector out-parameter",
                    )));
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
            _ => {
                return Err(LowerError::UnsupportedType(format!(
                    "LPFX argument type {:?} (scalar stage)",
                    module.types[arg.ty].inner
                )));
            }
        }
    }
    let return_types = if let Some(res) = &f.result {
        match &module.types[res.ty].inner {
            TypeInner::Scalar(scalar) => vec![naga_scalar_to_ir_type(scalar.kind)?],
            _ => {
                return Err(LowerError::UnsupportedType(String::from(
                    "LPFX non-scalar return",
                )));
            }
        }
    } else {
        Vec::new()
    };
    let func_name = format!("{name}_{}", callee.index());
    Ok(ImportDecl {
        module_name: String::from("lpfx"),
        func_name,
        param_types,
        return_types,
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
                TypeInner::Vector { .. } => {
                    return Err(LowerError::UnsupportedType(String::from(
                        "LPFX vector out-parameter",
                    )));
                }
                _ => {
                    return Err(LowerError::UnsupportedType(String::from(
                        "LPFX out-pointer",
                    )));
                }
            },
            TypeInner::Scalar(scalar) => {
                let _ = naga_scalar_to_ir_type(scalar.kind)?;
                out.push(LpfxArgKind::Value);
            }
            _ => {
                return Err(LowerError::UnsupportedType(String::from(
                    "LPFX non-scalar value arg",
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
    // (slot base address vreg, destination local vreg, scalar type stored in slot)
    let mut outs: Vec<(VReg, VReg, IrType)> = Vec::new();

    for (kind, &arg_expr) in kinds.iter().zip(arguments.iter()) {
        match kind {
            LpfxArgKind::Value => {
                arg_vs.push(ctx.ensure_expr(arg_expr)?);
            }
            LpfxArgKind::OutScalar(ir) => {
                let lv = out_pointer_local(ctx.func, arg_expr)?;
                let dst = ctx.resolve_local(lv)?;
                let slot = ctx.fb.alloc_slot(4);
                let addr = ctx.fb.alloc_vreg(IrType::I32);
                ctx.fb.push(Op::SlotAddr { dst: addr, slot });
                arg_vs.push(addr);
                outs.push((addr, dst, *ir));
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
        let ir_ty = match inner {
            TypeInner::Scalar(scalar) => naga_scalar_to_ir_type(scalar.kind)?,
            _ => {
                return Err(LowerError::Internal(String::from(
                    "LPFX non-scalar return value",
                )));
            }
        };
        let v = ctx.fb.alloc_vreg(ir_ty);
        if let Some(slot) = ctx.expr_cache.get_mut(res_h.index()) {
            *slot = Some(v);
        }
        result_vs.push(v);
    } else if f.result.is_some() {
        return Err(LowerError::Internal(String::from(
            "LPFX scalar return missing result expression",
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

    Ok(())
}
