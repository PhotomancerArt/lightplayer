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
    emit_abs_top_of_stack, emit_binary, emit_cast, emit_expr, emit_global_expression,
    emit_round_top_of_stack, emit_scalar_min_max, emit_unary, emit_zero_value_inner,
    expr_scalar_kind,
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
        Expression::Math {
            fun: MathFunction::Mix | MathFunction::SmoothStep,
            arg,
            arg1: Some(a1),
            arg2: Some(a2),
            ..
        } => {
            let la = expr_component_count(module, func, *arg)?;
            let lb = expr_component_count(module, func, *a1)?;
            let lc = expr_component_count(module, func, *a2)?;
            ternary_broadcast_dim(la, lb, lc)
        }
        Expression::Math {
            fun: MathFunction::Step,
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
                    "WASM codegen: step vector dimension mismatch {la} vs {lb}"
                ))
            }
        }
        Expression::Math {
            fun: MathFunction::Round,
            arg,
            ..
        } => expr_component_count(module, func, *arg),
        Expression::Math {
            fun: MathFunction::Abs,
            arg,
            ..
        } => expr_component_count(module, func, *arg),
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
        Expression::Math {
            fun: MathFunction::Mix | MathFunction::SmoothStep,
            arg,
            ..
        } => vector_element_kind(module, func, *arg),
        Expression::Math {
            fun: MathFunction::Step,
            arg,
            ..
        } => vector_element_kind(module, func, *arg),
        Expression::Math {
            fun: MathFunction::Round,
            arg,
            ..
        } => vector_element_kind(module, func, *arg),
        Expression::Math {
            fun: MathFunction::Abs,
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
        Expression::Math {
            fun: MathFunction::Mix,
            arg,
            arg1: Some(y),
            arg2: Some(t),
            ..
        } => emit_vector_mix(module, func, *arg, *y, *t, wasm_fn, mode, alloc),
        Expression::Math {
            fun: MathFunction::SmoothStep,
            arg: e0,
            arg1: Some(e1),
            arg2: Some(x),
            ..
        } => emit_vector_smoothstep(module, func, *e0, *e1, *x, wasm_fn, mode, alloc),
        Expression::Math {
            fun: MathFunction::Step,
            arg: edge,
            arg1: Some(x),
            ..
        } => emit_vector_step(module, func, *edge, *x, wasm_fn, mode, alloc),
        Expression::Math {
            fun: MathFunction::Round,
            arg,
            ..
        } => {
            let k = vector_element_kind(module, func, *arg)?;
            if k != ScalarKind::Float {
                return Err(String::from("WASM codegen: round expects float vector"));
            }
            let dim = expr_component_count(module, func, *arg)?;
            emit_expr(module, func, *arg, wasm_fn, mode, alloc)?;
            let base = alloc.alloc_temp_n(dim)?;
            for i in (0..dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(base + i));
            }
            for i in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(base + i));
                emit_round_top_of_stack(wasm_fn, mode, alloc)?;
            }
            Ok(())
        }
        Expression::Math {
            fun: MathFunction::Abs,
            arg,
            ..
        } => {
            let k = vector_element_kind(module, func, *arg)?;
            if k != ScalarKind::Float {
                return Err(String::from("WASM codegen: abs expects float vector"));
            }
            let dim = expr_component_count(module, func, *arg)?;
            emit_expr(module, func, *arg, wasm_fn, mode, alloc)?;
            let base = alloc.alloc_temp_n(dim)?;
            for i in (0..dim).rev() {
                wasm_fn.instruction(&Instruction::LocalSet(base + i));
            }
            for i in 0..dim {
                wasm_fn.instruction(&Instruction::LocalGet(base + i));
                emit_abs_top_of_stack(wasm_fn, mode, alloc)?;
            }
            Ok(())
        }
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

fn ternary_broadcast_dim(la: u32, lb: u32, lc: u32) -> Result<u32, String> {
    if la == lb && lb == lc {
        return Ok(la);
    }
    let dim = la.max(lb).max(lc);
    if dim == 0 {
        return Err(String::from("WASM codegen: mix/smoothstep zero dimension"));
    }
    if (la != 1 && la != dim) || (lb != 1 && lb != dim) || (lc != 1 && lc != dim) {
        return Err(format!(
            "WASM codegen: mix/smoothstep broadcast mismatch {la} {lb} {lc} vs dim {dim}"
        ));
    }
    Ok(dim)
}

fn emit_vector_mix(
    module: &Module,
    func: &Function,
    x: Handle<Expression>,
    y: Handle<Expression>,
    t: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let lx = expr_component_count(module, func, x)?;
    let ly = expr_component_count(module, func, y)?;
    let lt = expr_component_count(module, func, t)?;
    let dim = ternary_broadcast_dim(lx, ly, lt)?;
    let k = vector_element_kind(module, func, x)?;
    if k != ScalarKind::Float {
        return Err(String::from("WASM codegen: vector mix expects float"));
    }

    emit_maybe_splat(module, func, x, lx, dim, wasm_fn, mode, alloc)?;
    emit_maybe_splat(module, func, y, ly, dim, wasm_fn, mode, alloc)?;
    emit_maybe_splat(module, func, t, lt, dim, wasm_fn, mode, alloc)?;

    let pool = alloc.alloc_temp_n(3 * dim)?;
    let tb = pool + 2 * dim;
    let yb = pool + dim;
    let xb = pool;
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(tb + i));
    }
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(yb + i));
    }
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(xb + i));
    }
    for i in 0..dim {
        wasm_fn.instruction(&Instruction::LocalGet(yb + i));
        wasm_fn.instruction(&Instruction::LocalGet(xb + i));
        match mode {
            FloatMode::Float => {
                wasm_fn.instruction(&Instruction::F32Sub);
                wasm_fn.instruction(&Instruction::LocalGet(tb + i));
                wasm_fn.instruction(&Instruction::F32Mul);
                wasm_fn.instruction(&Instruction::LocalGet(xb + i));
                wasm_fn.instruction(&Instruction::F32Add);
            }
            FloatMode::Q32 => {
                crate::emit::emit_q32_sub_sat(wasm_fn, alloc)?;
                wasm_fn.instruction(&Instruction::LocalGet(tb + i));
                crate::emit::emit_q32_mul(wasm_fn, alloc)?;
                wasm_fn.instruction(&Instruction::LocalGet(xb + i));
                crate::emit::emit_q32_add_sat(wasm_fn, alloc)?;
            }
        }
    }
    Ok(())
}

fn emit_vector_smoothstep(
    module: &Module,
    func: &Function,
    e0: Handle<Expression>,
    e1: Handle<Expression>,
    x: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let l0 = expr_component_count(module, func, e0)?;
    let l1 = expr_component_count(module, func, e1)?;
    let lx = expr_component_count(module, func, x)?;
    let dim = ternary_broadcast_dim(l0, l1, lx)?;
    let k = vector_element_kind(module, func, e0)?;
    if k != ScalarKind::Float {
        return Err(String::from(
            "WASM codegen: vector smoothstep expects float",
        ));
    }

    emit_maybe_splat(module, func, e0, l0, dim, wasm_fn, mode, alloc)?;
    emit_maybe_splat(module, func, e1, l1, dim, wasm_fn, mode, alloc)?;
    emit_maybe_splat(module, func, x, lx, dim, wasm_fn, mode, alloc)?;

    let vec_slots = 3 * dim;
    let step_slots = 7u32;
    if vec_slots.checked_add(step_slots).unwrap_or(u32::MAX) > LocalAlloc::scratch_pool_len() {
        return Err(format!(
            "WASM codegen: smoothstep vec{dim} needs {} scratch locals (max {})",
            vec_slots + step_slots,
            LocalAlloc::scratch_pool_len()
        ));
    }
    let _ = alloc.alloc_temp_n(LocalAlloc::scratch_pool_len())?;
    let base = alloc.scratch_pool_base();
    let e0b = base;
    let e1b = base + dim;
    let xb = base + 2 * dim;
    let work = base + vec_slots;
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(xb + i));
    }
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(e1b + i));
    }
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(e0b + i));
    }
    for i in 0..dim {
        wasm_fn.instruction(&Instruction::LocalGet(e0b + i));
        wasm_fn.instruction(&Instruction::LocalSet(work));
        wasm_fn.instruction(&Instruction::LocalGet(e1b + i));
        wasm_fn.instruction(&Instruction::LocalSet(work + 1));
        wasm_fn.instruction(&Instruction::LocalGet(xb + i));
        wasm_fn.instruction(&Instruction::LocalSet(work + 2));
        crate::emit::emit_smoothstep_e0_e1_x_slots(wasm_fn, mode, alloc, work)?;
    }
    Ok(())
}

fn emit_vector_step(
    module: &Module,
    func: &Function,
    edge: Handle<Expression>,
    x: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let le = expr_component_count(module, func, edge)?;
    let lx = expr_component_count(module, func, x)?;
    let dim = if le == lx {
        le
    } else if le == 1 {
        lx
    } else if lx == 1 {
        le
    } else {
        return Err(format!(
            "WASM codegen: step vector dimension mismatch {le} vs {lx}"
        ));
    };
    let k = vector_element_kind(module, func, edge)?;
    if k != ScalarKind::Float {
        return Err(String::from("WASM codegen: vector step expects float"));
    }

    emit_maybe_splat(module, func, edge, le, dim, wasm_fn, mode, alloc)?;
    emit_maybe_splat(module, func, x, lx, dim, wasm_fn, mode, alloc)?;

    let pool = alloc.alloc_temp_n(2 * dim)?;
    let xb = pool + dim;
    let eb = pool;
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(xb + i));
    }
    for i in (0..dim).rev() {
        wasm_fn.instruction(&Instruction::LocalSet(eb + i));
    }
    for i in 0..dim {
        wasm_fn.instruction(&Instruction::LocalGet(xb + i));
        wasm_fn.instruction(&Instruction::LocalGet(eb + i));
        crate::emit::emit_step_x_edge_stack(wasm_fn, mode, alloc)?;
    }
    Ok(())
}

fn emit_maybe_splat(
    module: &Module,
    func: &Function,
    expr_h: Handle<Expression>,
    lc: u32,
    dim: u32,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    if lc == 1 && dim > 1 {
        emit_expr(module, func, expr_h, wasm_fn, mode, alloc)?;
        let s = alloc.splat_scratch;
        wasm_fn.instruction(&Instruction::LocalSet(s));
        for _ in 0..dim {
            wasm_fn.instruction(&Instruction::LocalGet(s));
        }
        Ok(())
    } else {
        emit_expr(module, func, expr_h, wasm_fn, mode, alloc)
    }
}

fn swizzle_component_index(c: SwizzleComponent) -> u32 {
    match c {
        SwizzleComponent::X => 0,
        SwizzleComponent::Y => 1,
        SwizzleComponent::Z => 2,
        SwizzleComponent::W => 3,
    }
}
