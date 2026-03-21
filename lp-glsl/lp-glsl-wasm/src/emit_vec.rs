//! Scalarized vector lowering: N stack values per `vecN`.

use alloc::format;
use alloc::string::String;

use lp_glsl_naga::FloatMode;
use naga::{
    BinaryOperator, Expression, Function, Handle, MathFunction, Module, ScalarKind,
    SwizzleComponent, TypeInner,
};
use wasm_encoder::{Function as WasmFunction, Instruction};

use crate::emit::{
    emit_binary, emit_cast, emit_expr, emit_global_expression, emit_scalar_min_max, emit_unary,
    emit_zero_value_inner, expr_scalar_kind,
};
use crate::locals::LocalAlloc;
use crate::types::{type_handle_component_count, type_handle_element_scalar_kind};

pub(crate) fn expr_component_count(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Result<u32, String> {
    match &func.expressions[expr] {
        Expression::Literal(_) => Ok(1),
        Expression::Constant(h) => Ok(type_handle_component_count(module, module.constants[*h].ty)),
        Expression::ZeroValue(ty_h) => Ok(type_handle_component_count(module, *ty_h)),
        Expression::Compose { ty, .. } => Ok(type_handle_component_count(module, *ty)),
        Expression::Splat { size, .. } => Ok(*size as u32),
        Expression::Swizzle { size, .. } => Ok(*size as u32),
        Expression::AccessIndex { .. } => Ok(1),
        Expression::Access { .. } => Ok(1),
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| String::from("bad argument index"))?;
            Ok(type_handle_component_count(module, arg.ty))
        }
        Expression::LocalVariable(lv) => Ok(type_handle_component_count(
            module,
            func.local_variables[*lv].ty,
        )),
        Expression::Load { pointer } => {
            let ptr = &func.expressions[*pointer];
            match ptr {
                Expression::LocalVariable(lv) => Ok(type_handle_component_count(
                    module,
                    func.local_variables[*lv].ty,
                )),
                _ => expr_component_count(module, func, *pointer),
            }
        }
        Expression::Binary { left, right, .. } => {
            let lc = expr_component_count(module, func, *left)?;
            let rc = expr_component_count(module, func, *right)?;
            if lc == rc {
                Ok(lc)
            } else if lc == 1 {
                Ok(rc)
            } else if rc == 1 {
                Ok(lc)
            } else {
                Err(format!(
                    "WASM codegen: binary vector dimension mismatch {lc} vs {rc}"
                ))
            }
        }
        Expression::Unary { expr: inner, .. } => expr_component_count(module, func, *inner),
        Expression::Select { accept, .. } => expr_component_count(module, func, *accept),
        Expression::As { expr: inner, .. } => expr_component_count(module, func, *inner),
        Expression::CallResult(fh) => {
            let ret = module.functions[*fh]
                .result
                .as_ref()
                .ok_or_else(|| String::from("CallResult void"))?;
            Ok(type_handle_component_count(module, ret.ty))
        }
        Expression::Math {
            fun: MathFunction::Min | MathFunction::Max,
            arg,
            arg1: Some(arg1),
            ..
        } => {
            let la = expr_component_count(module, func, *arg)?;
            let lb = expr_component_count(module, func, *arg1)?;
            if la == lb {
                Ok(la)
            } else if la == 1 {
                Ok(lb)
            } else if lb == 1 {
                Ok(la)
            } else {
                Err(format!(
                    "WASM codegen: min/max vector dimension mismatch {la} vs {lb}"
                ))
            }
        }
        Expression::Relational { .. } | Expression::Math { .. } => Ok(1),
        _ => Err(format!(
            "WASM codegen: cannot infer component count for {:?}",
            func.expressions[expr]
        )),
    }
}

pub(crate) fn vector_element_kind(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Result<ScalarKind, String> {
    match &func.expressions[expr] {
        Expression::Constant(h) => type_handle_element_scalar_kind(module, module.constants[*h].ty)
            .map_err(|e| String::from(e)),
        Expression::Compose { ty, .. } => {
            type_handle_element_scalar_kind(module, *ty).map_err(|e| String::from(e))
        }
        Expression::Splat { value, .. } => expr_scalar_kind(module, func, *value),
        Expression::Swizzle { vector, .. } => vector_element_kind(module, func, *vector),
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| String::from("bad argument index"))?;
            type_handle_element_scalar_kind(module, arg.ty).map_err(|e| String::from(e))
        }
        Expression::Load { pointer } => {
            let ptr = &func.expressions[*pointer];
            match ptr {
                Expression::LocalVariable(lv) => {
                    type_handle_element_scalar_kind(module, func.local_variables[*lv].ty)
                        .map_err(|e| String::from(e))
                }
                _ => vector_element_kind(module, func, *pointer),
            }
        }
        Expression::Binary { left, .. } => vector_element_kind(module, func, *left),
        Expression::Unary { expr: inner, .. } => vector_element_kind(module, func, *inner),
        Expression::Select { accept, .. } => vector_element_kind(module, func, *accept),
        Expression::As { kind, .. } => Ok(*kind),
        Expression::ZeroValue(ty_h) => {
            type_handle_element_scalar_kind(module, *ty_h).map_err(|e| String::from(e))
        }
        Expression::Math {
            fun: MathFunction::Min | MathFunction::Max,
            arg,
            ..
        } => vector_element_kind(module, func, *arg),
        _ => expr_scalar_kind(module, func, expr),
    }
}

pub(crate) fn emit_vector_expr(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match &func.expressions[expr] {
        Expression::Compose { components, .. } => {
            for &c in components {
                emit_expr(module, func, c, wasm_fn, mode, alloc)?;
            }
            Ok(())
        }
        Expression::Splat { size, value } => {
            let dim = *size as u32;
            emit_expr(module, func, *value, wasm_fn, mode, alloc)?;
            if dim <= 1 {
                return Ok(());
            }
            let scratch = alloc.splat_scratch;
            wasm_fn.instruction(&Instruction::LocalSet(scratch));
            for _ in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(scratch));
            }
            Ok(())
        }
        Expression::Constant(h) => {
            let init = module.constants[*h].init;
            emit_global_expression(module, init, wasm_fn, mode, alloc)
        }
        Expression::Swizzle {
            size,
            vector,
            pattern,
        } => {
            let src_dim = expr_component_count(module, func, *vector)?;
            let dst_dim = *size as u32;
            emit_expr(module, func, *vector, wasm_fn, mode, alloc)?;
            let base = alloc.alloc_temp_n(src_dim)?;
            for i in (0..src_dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(base + i));
            }
            for i in 0..dst_dim {
                let comp = pattern[i as usize];
                let idx = swizzle_component_index(comp);
                if idx >= src_dim {
                    return Err(format!(
                        "WASM codegen: swizzle index {idx} out of range for dim {src_dim}"
                    ));
                }
                wasm_fn.instruction(&Instruction::LocalGet(base + idx));
            }
            Ok(())
        }
        Expression::AccessIndex { base, index } => {
            let base_dim = expr_component_count(module, func, *base)?;
            if base_dim == 1 {
                return emit_expr(module, func, *base, wasm_fn, mode, alloc);
            }
            emit_expr(module, func, *base, wasm_fn, mode, alloc)?;
            let scratch_base = alloc.alloc_temp_n(base_dim)?;
            for i in (0..base_dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(scratch_base + i));
            }
            wasm_fn.instruction(&Instruction::LocalGet(scratch_base + *index));
            Ok(())
        }
        Expression::Math {
            fun: MathFunction::Min,
            arg,
            arg1: Some(arg1),
            ..
        } => emit_vector_min_max(module, func, *arg, *arg1, false, wasm_fn, mode, alloc),
        Expression::Math {
            fun: MathFunction::Max,
            arg,
            arg1: Some(arg1),
            ..
        } => emit_vector_min_max(module, func, *arg, *arg1, true, wasm_fn, mode, alloc),
        Expression::Binary { op, left, right } => {
            emit_vector_binary(module, func, *op, *left, *right, wasm_fn, mode, alloc)
        }
        Expression::Unary { op, expr: inner } => {
            let k = vector_element_kind(module, func, *inner)?;
            let dim = expr_component_count(module, func, *inner)?;
            emit_expr(module, func, *inner, wasm_fn, mode, alloc)?;
            let base = alloc.alloc_temp_n(dim)?;
            for i in (0..dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(base + i));
            }
            for i in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(base + i));
                emit_unary(*op, k, mode, wasm_fn)?;
            }
            Ok(())
        }
        Expression::Select {
            condition,
            accept,
            reject,
        } => {
            let dim = expr_component_count(module, func, *accept)?;
            let cond_dim = expr_component_count(module, func, *condition)?;
            if cond_dim != dim {
                return Err(format!(
                    "WASM codegen: select condition dim {cond_dim} vs accept dim {dim}"
                ));
            }
            emit_expr(module, func, *accept, wasm_fn, mode, alloc)?;
            let pool = alloc.alloc_temp_n(3 * dim)?;
            let acc_base = pool;
            let rej_base = pool + dim;
            let cond_base = pool + 2 * dim;
            for i in (0..dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(acc_base + i));
            }
            emit_expr(module, func, *reject, wasm_fn, mode, alloc)?;
            for i in (0..dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(rej_base + i));
            }
            emit_expr(module, func, *condition, wasm_fn, mode, alloc)?;
            for i in (0..dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(cond_base + i));
            }
            for i in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(acc_base + i));
                wasm_fn.instruction(&Instruction::LocalGet(rej_base + i));
                wasm_fn.instruction(&Instruction::LocalGet(cond_base + i));
                wasm_fn.instruction(&Instruction::Select);
            }
            Ok(())
        }
        Expression::As {
            expr: inner,
            kind,
            convert,
        } => {
            let src_k = vector_element_kind(module, func, *inner)?;
            let dim = expr_component_count(module, func, *inner)?;
            emit_expr(module, func, *inner, wasm_fn, mode, alloc)?;
            let base = alloc.alloc_temp_n(dim)?;
            for i in (0..dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(base + i));
            }
            for i in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(base + i));
                emit_cast(*kind, *convert, src_k, mode, wasm_fn, alloc)?;
            }
            Ok(())
        }
        Expression::ZeroValue(ty_h) => {
            let inner = &module.types[*ty_h].inner;
            if let TypeInner::Vector { size, scalar } = *inner {
                let dim = size as u32;
                for _ in 0..dim {
                    emit_zero_value_inner(&TypeInner::Scalar(scalar), wasm_fn, mode)?;
                }
                Ok(())
            } else {
                Err(String::from(
                    "WASM codegen: ZeroValue vector expected Vector inner",
                ))
            }
        }
        Expression::CallResult(_) => {
            let base = alloc
                .call_result_wasm_base(expr)
                .ok_or_else(|| String::from("WASM codegen: CallResult local missing"))?;
            let dim = expr_component_count(module, func, expr)?;
            for k in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(base + k));
            }
            Ok(())
        }
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| String::from("bad argument index"))?;
            let dim = type_handle_component_count(module, arg.ty);
            let wasm_idx = alloc
                .function_argument_wasm_base(*i)
                .ok_or_else(|| String::from("bad argument wasm base"))?;
            for k in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(wasm_idx + k));
            }
            Ok(())
        }
        Expression::Load { pointer } => {
            let ptr = &func.expressions[*pointer];
            match ptr {
                Expression::LocalVariable(lv) => {
                    let idx = alloc
                        .resolve_local_variable(*lv)
                        .ok_or_else(|| String::from("WASM codegen: unresolved local variable"))?;
                    let dim = alloc.local_variable_slots(module, func, *lv);
                    for k in 0..dim {
                        wasm_fn.instruction(&Instruction::LocalGet(idx + k));
                    }
                    Ok(())
                }
                _ => Err(String::from(
                    "WASM codegen: vector load from non-local pointer",
                )),
            }
        }
        _ => Err(format!(
            "WASM codegen: unsupported multi-component expression {:?}",
            func.expressions[expr]
        )),
    }
}

fn emit_vector_min_max(
    module: &Module,
    func: &Function,
    left: Handle<Expression>,
    right: Handle<Expression>,
    is_max: bool,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let lc = expr_component_count(module, func, left)?;
    let rc = expr_component_count(module, func, right)?;
    let dim = if lc == rc {
        lc
    } else if lc == 1 {
        rc
    } else if rc == 1 {
        lc
    } else {
        return Err(format!(
            "WASM codegen: min/max vector dimension mismatch {lc} vs {rc}"
        ));
    };
    let k = vector_element_kind(module, func, left)?;

    if lc == 1 && dim > 1 {
        emit_expr(module, func, left, wasm_fn, mode, alloc)?;
        let s = alloc.splat_scratch;
        wasm_fn.instruction(&Instruction::LocalSet(s));
        for _ in 0..dim {
            wasm_fn.instruction(&Instruction::LocalGet(s));
        }
    } else {
        emit_expr(module, func, left, wasm_fn, mode, alloc)?;
    }

    if rc == 1 && dim > 1 {
        emit_expr(module, func, right, wasm_fn, mode, alloc)?;
        let s = alloc.splat_scratch;
        wasm_fn.instruction(&Instruction::LocalSet(s));
        for _ in 0..dim {
            wasm_fn.instruction(&Instruction::LocalGet(s));
        }
    } else {
        emit_expr(module, func, right, wasm_fn, mode, alloc)?;
    }

    let pool = alloc.alloc_temp_n(2 * dim)?;
    let right_base = pool + dim;
    let left_base = pool;
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(right_base + i));
    }
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(left_base + i));
    }
    for i in 0..dim {
        wasm_fn.instruction(&Instruction::LocalGet(left_base + i));
        wasm_fn.instruction(&Instruction::LocalGet(right_base + i));
        emit_scalar_min_max(is_max, k, mode, wasm_fn, alloc)?;
    }
    Ok(())
}

fn emit_vector_binary(
    module: &Module,
    func: &Function,
    op: BinaryOperator,
    left: Handle<Expression>,
    right: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let lc = expr_component_count(module, func, left)?;
    let rc = expr_component_count(module, func, right)?;
    let dim = if lc == rc {
        lc
    } else if lc == 1 {
        rc
    } else if rc == 1 {
        lc
    } else {
        return Err(format!(
            "WASM codegen: vector binary dimension mismatch {lc} vs {rc}"
        ));
    };
    let k = vector_element_kind(module, func, left)?;

    if lc == 1 && dim > 1 {
        emit_expr(module, func, left, wasm_fn, mode, alloc)?;
        let s = alloc.splat_scratch;
        wasm_fn.instruction(&Instruction::LocalSet(s));
        for _ in 0..dim {
            wasm_fn.instruction(&Instruction::LocalGet(s));
        }
    } else {
        emit_expr(module, func, left, wasm_fn, mode, alloc)?;
    }

    if rc == 1 && dim > 1 {
        emit_expr(module, func, right, wasm_fn, mode, alloc)?;
        let s = alloc.splat_scratch;
        wasm_fn.instruction(&Instruction::LocalSet(s));
        for _ in 0..dim {
            wasm_fn.instruction(&Instruction::LocalGet(s));
        }
    } else {
        emit_expr(module, func, right, wasm_fn, mode, alloc)?;
    }

    let pool = alloc.alloc_temp_n(2 * dim)?;
    let right_base = pool + dim;
    let left_base = pool;
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(right_base + i));
    }
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(left_base + i));
    }
    for i in 0..dim {
        wasm_fn.instruction(&Instruction::LocalGet(left_base + i));
        wasm_fn.instruction(&Instruction::LocalGet(right_base + i));
        emit_binary(op, k, mode, wasm_fn, alloc)?;
    }
    Ok(())
}

fn swizzle_component_index(c: SwizzleComponent) -> u32 {
    match c {
        SwizzleComponent::X => 0,
        SwizzleComponent::Y => 1,
        SwizzleComponent::Z => 2,
        SwizzleComponent::W => 3,
    }
}
