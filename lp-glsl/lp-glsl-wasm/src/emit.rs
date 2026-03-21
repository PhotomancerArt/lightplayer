//! Lower `naga::Module` to a WASM binary via `wasm-encoder`.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use lp_glsl_naga::{FloatMode, NagaModule};
use naga::{
    BinaryOperator, Block, Bytes, Expression, Function, Handle, Literal, Module, ScalarKind,
    Statement, TypeInner, UnaryOperator,
};
use wasm_encoder::{
    BlockType, CodeSection, ExportKind, ExportSection, Function as WasmFunction, FunctionSection,
    Ieee32, Instruction, Module as WasmModule, TypeSection, ValType,
};

use crate::locals::LocalAlloc;
use crate::options::WasmOptions;
use crate::types::glsl_type_to_wasm_components;

const Q16_16_SCALE: f32 = 65536.0;

pub fn emit_module(naga_module: &NagaModule, options: &WasmOptions) -> Result<Vec<u8>, String> {
    let module = &naga_module.module;
    let mode = options.float_mode;

    let mut types_sec = TypeSection::new();
    let mut func_sec = FunctionSection::new();
    let mut export_sec = ExportSection::new();
    let mut code_sec = CodeSection::new();

    for (func_i, (func_handle, fi)) in naga_module.functions.iter().enumerate() {
        let func = &module.functions[*func_handle];

        let name = func.name.as_deref().unwrap_or("_unnamed");
        let params: Vec<ValType> = fi
            .params
            .iter()
            .flat_map(|(_, ty)| glsl_type_to_wasm_components(ty, mode))
            .collect();
        let results: Vec<ValType> = glsl_type_to_wasm_components(&fi.return_type, mode);

        let type_idx = func_i as u32;
        types_sec.ty().function(params.clone(), results.clone());
        func_sec.function(type_idx);
        export_sec.export(name, ExportKind::Func, func_i as u32);

        let alloc = LocalAlloc::new(module, func, mode);
        let locals = alloc.wasm_local_groups();
        let mut wasm_fn = WasmFunction::new(locals);

        emit_local_inits(module, func, &mut wasm_fn, mode, &alloc)?;

        emit_block(module, func, &func.body, &mut wasm_fn, mode, &alloc)?;

        wasm_fn.instruction(&Instruction::End);
        code_sec.function(&wasm_fn);
    }

    let mut out = WasmModule::new();
    out.section(&types_sec);
    out.section(&func_sec);
    out.section(&export_sec);
    out.section(&code_sec);
    Ok(out.finish())
}

/// Naga keeps `LocalVariable::init` instead of always lowering to `Store`; emit those before the body.
fn emit_local_inits(
    module: &Module,
    func: &Function,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    for (handle, lv) in func.local_variables.iter() {
        if alloc.is_parameter_alias(handle) {
            continue;
        }
        let Some(init_h) = lv.init else {
            continue;
        };
        emit_expr(module, func, init_h, wasm_fn, mode, alloc)?;
        let idx = alloc
            .resolve_local_variable(handle)
            .ok_or_else(|| String::from("WASM codegen: init for unresolved local"))?;
        wasm_fn.instruction(&Instruction::LocalSet(idx));
    }
    Ok(())
}

fn emit_block(
    module: &Module,
    func: &Function,
    block: &Block,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    for stmt in block.iter() {
        emit_stmt(module, func, stmt, wasm_fn, mode, alloc)?;
    }
    Ok(())
}

fn emit_stmt(
    module: &Module,
    func: &Function,
    stmt: &Statement,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match stmt {
        Statement::Emit(range) => {
            for h in range.clone() {
                emit_expr(module, func, h, wasm_fn, mode, alloc)?;
                wasm_fn.instruction(&Instruction::Drop);
            }
            Ok(())
        }
        Statement::Block(inner) => emit_block(module, func, inner, wasm_fn, mode, alloc),
        Statement::If {
            condition,
            accept,
            reject,
        } => {
            emit_expr(module, func, *condition, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::If(BlockType::Empty));
            emit_block(module, func, accept, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::Else);
            emit_block(module, func, reject, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::End);
            Ok(())
        }
        Statement::Loop {
            body,
            continuing,
            break_if,
        } => {
            wasm_fn.instruction(&Instruction::Block(BlockType::Empty));
            wasm_fn.instruction(&Instruction::Loop(BlockType::Empty));
            wasm_fn.instruction(&Instruction::Block(BlockType::Empty));
            emit_block(module, func, body, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::End);
            emit_block(module, func, continuing, wasm_fn, mode, alloc)?;
            if let Some(h) = break_if {
                emit_expr(module, func, *h, wasm_fn, mode, alloc)?;
                wasm_fn.instruction(&Instruction::BrIf(1));
            }
            wasm_fn.instruction(&Instruction::Br(0));
            wasm_fn.instruction(&Instruction::End);
            wasm_fn.instruction(&Instruction::End);
            Ok(())
        }
        Statement::Break | Statement::Continue => Err(String::from(
            "WASM codegen: break/continue not supported (unexpected in scalar filetests)",
        )),
        Statement::Return { value } => {
            match value {
                Some(h) => emit_expr(module, func, *h, wasm_fn, mode, alloc)?,
                None => {}
            }
            wasm_fn.instruction(&Instruction::Return);
            Ok(())
        }
        Statement::Store { pointer, value } => {
            emit_expr(module, func, *value, wasm_fn, mode, alloc)?;
            let lv = store_pointer_local(func, *pointer)?;
            let idx = alloc
                .resolve_local_variable(lv)
                .ok_or_else(|| String::from("WASM codegen: store to unresolved local"))?;
            wasm_fn.instruction(&Instruction::LocalSet(idx));
            Ok(())
        }
        _ => Err(format!("WASM codegen: unsupported statement {stmt:?}")),
    }
}

fn store_pointer_local(
    func: &Function,
    pointer: Handle<Expression>,
) -> Result<Handle<naga::LocalVariable>, String> {
    match &func.expressions[pointer] {
        Expression::LocalVariable(lv) => Ok(*lv),
        _ => Err(String::from(
            "WASM codegen: store pointer must be a local variable for Phase I",
        )),
    }
}

fn emit_expr(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match &func.expressions[expr] {
        Expression::Literal(lit) => emit_literal(lit, wasm_fn, mode),
        Expression::FunctionArgument(i) => {
            wasm_fn.instruction(&Instruction::LocalGet(*i));
            Ok(())
        }
        Expression::LocalVariable(_) => Err(String::from(
            "WASM codegen: LocalVariable must be used through Load",
        )),
        Expression::Load { pointer } => {
            let ptr = &func.expressions[*pointer];
            match ptr {
                Expression::LocalVariable(lv) => {
                    let idx = alloc
                        .resolve_local_variable(*lv)
                        .ok_or_else(|| String::from("WASM codegen: unresolved local variable"))?;
                    wasm_fn.instruction(&Instruction::LocalGet(idx));
                    Ok(())
                }
                _ => Err(String::from("WASM codegen: load from non-local pointer")),
            }
        }
        Expression::Binary { op, left, right } => {
            let k = expr_scalar_kind(module, func, *left)?;
            emit_expr(module, func, *left, wasm_fn, mode, alloc)?;
            emit_expr(module, func, *right, wasm_fn, mode, alloc)?;
            emit_binary(*op, k, mode, wasm_fn, alloc)?;
            Ok(())
        }
        Expression::Unary { op, expr: inner } => {
            let k = expr_scalar_kind(module, func, *inner)?;
            emit_expr(module, func, *inner, wasm_fn, mode, alloc)?;
            emit_unary(*op, k, mode, wasm_fn)?;
            Ok(())
        }
        Expression::Select {
            condition,
            accept,
            reject,
        } => {
            emit_expr(module, func, *accept, wasm_fn, mode, alloc)?;
            emit_expr(module, func, *reject, wasm_fn, mode, alloc)?;
            emit_expr(module, func, *condition, wasm_fn, mode, alloc)?;
            wasm_fn.instruction(&Instruction::Select);
            Ok(())
        }
        Expression::As {
            expr: inner,
            kind,
            convert,
        } => {
            let src_k = expr_scalar_kind(module, func, *inner)?;
            emit_expr(module, func, *inner, wasm_fn, mode, alloc)?;
            emit_cast(*kind, *convert, src_k, mode, wasm_fn, alloc)?;
            Ok(())
        }
        Expression::ZeroValue(ty_h) => {
            let inner = &module.types[*ty_h].inner;
            emit_zero_value(inner, wasm_fn, mode)
        }
        _ => Err(format!(
            "WASM codegen: unsupported expression {:?}",
            func.expressions[expr]
        )),
    }
}

fn emit_zero_value(
    inner: &TypeInner,
    wasm_fn: &mut WasmFunction,
    mode: FloatMode,
) -> Result<(), String> {
    match *inner {
        TypeInner::Scalar(s) => match s.kind {
            ScalarKind::Float if s.width == 4 => match mode {
                FloatMode::Float => {
                    wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(0.0f32)));
                }
                FloatMode::Q32 => {
                    wasm_fn.instruction(&Instruction::I32Const(0));
                }
            },
            ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool if s.width == 4 => {
                wasm_fn.instruction(&Instruction::I32Const(0));
            }
            _ => {
                return Err(format!("WASM codegen: ZeroValue unsupported scalar {s:?}"));
            }
        },
        _ => {
            return Err(String::from(
                "WASM codegen: ZeroValue only for scalars in Phase I",
            ));
        }
    }
    Ok(())
}

/// Fixed16x16 values are clamped to approximately ±32768 (see filetests).
fn clamp_f32_to_q16_16_range(v: f32) -> f32 {
    const LO: f32 = -32768.0;
    const HI: f32 = 32767.9999847412109375;
    v.clamp(LO, HI)
}

fn emit_literal(lit: &Literal, wasm_fn: &mut WasmFunction, mode: FloatMode) -> Result<(), String> {
    match *lit {
        Literal::F32(v) => match mode {
            FloatMode::Float => {
                wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(v)));
                Ok(())
            }
            FloatMode::Q32 => {
                let v = clamp_f32_to_q16_16_range(v);
                let q = (f64::from(v) * f64::from(Q16_16_SCALE)) as i32;
                wasm_fn.instruction(&Instruction::I32Const(q));
                Ok(())
            }
        },
        Literal::I32(v) => {
            wasm_fn.instruction(&Instruction::I32Const(v));
            Ok(())
        }
        Literal::U32(v) => {
            wasm_fn.instruction(&Instruction::I32Const(v as i32));
            Ok(())
        }
        Literal::Bool(b) => {
            wasm_fn.instruction(&Instruction::I32Const(b as i32));
            Ok(())
        }
        _ => Err(format!("WASM codegen: unsupported literal {lit:?}")),
    }
}

fn expr_scalar_kind(
    module: &Module,
    func: &Function,
    expr: Handle<Expression>,
) -> Result<ScalarKind, String> {
    match &func.expressions[expr] {
        Expression::Literal(l) => match l {
            Literal::F32(_) | Literal::F64(_) | Literal::F16(_) | Literal::AbstractFloat(_) => {
                Ok(ScalarKind::Float)
            }
            Literal::I32(_) | Literal::I64(_) | Literal::AbstractInt(_) => Ok(ScalarKind::Sint),
            Literal::U32(_) | Literal::U64(_) => Ok(ScalarKind::Uint),
            Literal::Bool(_) => Ok(ScalarKind::Bool),
        },
        Expression::FunctionArgument(i) => {
            let arg = func
                .arguments
                .get(*i as usize)
                .ok_or_else(|| String::from("bad argument index"))?;
            type_handle_scalar_kind(module, arg.ty)
        }
        Expression::LocalVariable(lv) => {
            let lv_ty = func.local_variables[*lv].ty;
            type_handle_scalar_kind(module, lv_ty)
        }
        Expression::Load { pointer } => {
            let ptr = &func.expressions[*pointer];
            match ptr {
                Expression::LocalVariable(lv) => {
                    let inner = &module.types[func.local_variables[*lv].ty].inner;
                    match *inner {
                        TypeInner::Pointer { base, .. } => type_handle_scalar_kind(module, base),
                        TypeInner::ValuePointer { scalar, .. } => Ok(scalar.kind),
                        TypeInner::Scalar(s) => Ok(s.kind),
                        _ => Err(format!("load pointer type: {inner:?}")),
                    }
                }
                _ => expr_scalar_kind(module, func, *pointer),
            }
        }
        Expression::Binary { left, .. } => expr_scalar_kind(module, func, *left),
        Expression::Unary { expr: inner, .. } => expr_scalar_kind(module, func, *inner),
        Expression::Select { accept, .. } => expr_scalar_kind(module, func, *accept),
        Expression::As { kind, .. } => Ok(*kind),
        Expression::ZeroValue(ty_h) => match &module.types[*ty_h].inner {
            TypeInner::Scalar(s) => Ok(s.kind),
            _ => Err(String::from("zerovalue kind")),
        },
        _ => Err(format!(
            "cannot infer scalar kind for {:?}",
            func.expressions[expr]
        )),
    }
}

fn type_handle_scalar_kind(module: &Module, ty: Handle<naga::Type>) -> Result<ScalarKind, String> {
    match &module.types[ty].inner {
        TypeInner::Scalar(s) => Ok(s.kind),
        TypeInner::Pointer { base, .. } => type_handle_scalar_kind(module, *base),
        TypeInner::ValuePointer { scalar, .. } => Ok(scalar.kind),
        _ => Err(String::from("non-scalar type handle")),
    }
}

fn emit_binary(
    op: BinaryOperator,
    kind: ScalarKind,
    mode: FloatMode,
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    match (op, kind, mode) {
        (BinaryOperator::Add, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Add);
        }
        (BinaryOperator::Add, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Add);
        }
        (BinaryOperator::Subtract, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Sub);
        }
        (BinaryOperator::Subtract, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Sub);
        }
        (BinaryOperator::Multiply, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Mul);
        }
        (BinaryOperator::Multiply, ScalarKind::Float, FloatMode::Q32) => {
            emit_q32_mul(wasm_fn, alloc)?;
        }
        (BinaryOperator::Divide, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Div);
        }
        (BinaryOperator::Divide, ScalarKind::Float, FloatMode::Q32) => {
            emit_q32_div(wasm_fn, alloc)?;
        }
        (BinaryOperator::Modulo, ScalarKind::Float, FloatMode::Float) => {
            emit_f32_mod(wasm_fn, alloc)?;
        }
        (BinaryOperator::Modulo, ScalarKind::Float, FloatMode::Q32) => {
            return Err(String::from(
                "WASM codegen: float modulo in Q32 not implemented",
            ));
        }

        (BinaryOperator::Add, ScalarKind::Sint | ScalarKind::Uint | ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Add);
        }
        (BinaryOperator::Subtract, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Sub);
        }
        (BinaryOperator::Multiply, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Mul);
        }
        (BinaryOperator::Divide, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32DivS);
        }
        (BinaryOperator::Divide, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32DivU);
        }
        (BinaryOperator::Modulo, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32RemS);
        }
        (BinaryOperator::Modulo, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32RemU);
        }

        (BinaryOperator::Equal, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Eq);
        }
        (BinaryOperator::Equal, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Eq);
        }
        (BinaryOperator::NotEqual, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Ne);
        }
        (BinaryOperator::NotEqual, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Ne);
        }
        (BinaryOperator::Less, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Lt);
        }
        (BinaryOperator::Less, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32LtS);
        }
        (BinaryOperator::LessEqual, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Le);
        }
        (BinaryOperator::LessEqual, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32LeS);
        }
        (BinaryOperator::Greater, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Gt);
        }
        (BinaryOperator::Greater, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32GtS);
        }
        (BinaryOperator::GreaterEqual, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Ge);
        }
        (BinaryOperator::GreaterEqual, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32GeS);
        }

        (BinaryOperator::Equal, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32Eq);
        }
        (BinaryOperator::NotEqual, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32Ne);
        }
        (BinaryOperator::Less, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32LtS);
        }
        (BinaryOperator::LessEqual, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32LeS);
        }
        (BinaryOperator::Greater, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32GtS);
        }
        (BinaryOperator::GreaterEqual, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32GeS);
        }

        (BinaryOperator::Equal, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Eq);
        }
        (BinaryOperator::NotEqual, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Ne);
        }
        (BinaryOperator::Less, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32LtU);
        }
        (BinaryOperator::LessEqual, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32LeU);
        }
        (BinaryOperator::Greater, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32GtU);
        }
        (BinaryOperator::GreaterEqual, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32GeU);
        }

        (BinaryOperator::Equal, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Eq);
        }
        (BinaryOperator::NotEqual, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Ne);
        }

        (BinaryOperator::LogicalAnd, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32And);
        }
        (BinaryOperator::LogicalOr, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Or);
        }

        (BinaryOperator::And, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32And);
        }
        (BinaryOperator::InclusiveOr, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Or);
        }
        (BinaryOperator::ExclusiveOr, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Xor);
        }

        (BinaryOperator::ShiftLeft, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Shl);
        }
        (BinaryOperator::ShiftRight, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32ShrS);
        }
        (BinaryOperator::ShiftRight, ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32ShrU);
        }

        _ => {
            return Err(format!(
                "WASM codegen: unsupported binary {op:?} for {kind:?} / {mode:?}"
            ));
        }
    }
    Ok(())
}

fn emit_unary(
    op: UnaryOperator,
    kind: ScalarKind,
    mode: FloatMode,
    wasm_fn: &mut WasmFunction,
) -> Result<(), String> {
    match (op, kind, mode) {
        (UnaryOperator::Negate, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Neg);
        }
        (UnaryOperator::Negate, ScalarKind::Float, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32Sub);
        }
        (UnaryOperator::Negate, ScalarKind::Sint, _) => {
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32Sub);
        }
        (UnaryOperator::LogicalNot, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Eqz);
        }
        (UnaryOperator::BitwiseNot, ScalarKind::Sint | ScalarKind::Uint, _) => {
            wasm_fn.instruction(&Instruction::I32Const(-1));
            wasm_fn.instruction(&Instruction::I32Xor);
        }
        _ => {
            return Err(format!(
                "WASM codegen: unsupported unary {op:?} for {kind:?}"
            ));
        }
    }
    Ok(())
}

fn emit_cast(
    dst_kind: ScalarKind,
    convert: Option<Bytes>,
    src_kind: ScalarKind,
    mode: FloatMode,
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    if convert.is_none() {
        return Ok(());
    }
    match (src_kind, dst_kind, mode) {
        (ScalarKind::Float, ScalarKind::Float, _)
        | (ScalarKind::Sint, ScalarKind::Sint, _)
        | (ScalarKind::Uint, ScalarKind::Uint, _)
        | (ScalarKind::Bool, ScalarKind::Bool, _) => {}
        (ScalarKind::Float, ScalarKind::Sint, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::I32TruncF32S);
        }
        (ScalarKind::Float, ScalarKind::Uint, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::I32TruncF32U);
        }
        (ScalarKind::Sint, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32ConvertI32S);
        }
        (ScalarKind::Uint, ScalarKind::Float, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32ConvertI32U);
        }

        (ScalarKind::Float, ScalarKind::Sint, FloatMode::Q32) => {
            emit_q32_fixed_trunc_toward_zero_to_i32(wasm_fn);
            let scratch = alloc
                .q32_scratch
                .map(|(a, _)| a)
                .ok_or_else(|| String::from("Q32 scratch missing"))?;
            emit_clamp_i32_to_q16_int_range(wasm_fn, scratch);
        }
        (ScalarKind::Float, ScalarKind::Uint, FloatMode::Q32) => {
            emit_q32_fixed_trunc_toward_zero_to_i32(wasm_fn);
        }
        (ScalarKind::Sint, ScalarKind::Float, FloatMode::Q32) => {
            emit_i32_clamp_then_q32_scale_sint(wasm_fn, alloc)?;
        }
        (ScalarKind::Uint, ScalarKind::Float, FloatMode::Q32) => {
            emit_u32_clamp_then_q32_scale_uint(wasm_fn, alloc)?;
        }

        (ScalarKind::Sint, ScalarKind::Uint, _) | (ScalarKind::Uint, ScalarKind::Sint, _) => {}

        (ScalarKind::Bool, ScalarKind::Sint | ScalarKind::Uint, _) => {}

        (ScalarKind::Sint | ScalarKind::Uint, ScalarKind::Bool, _) => {
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32Ne);
        }

        (ScalarKind::Float, ScalarKind::Bool, FloatMode::Float) => {
            wasm_fn.instruction(&Instruction::F32Const(Ieee32::from(0.0f32)));
            wasm_fn.instruction(&Instruction::F32Ne);
        }
        (ScalarKind::Float, ScalarKind::Bool, FloatMode::Q32) => {
            wasm_fn.instruction(&Instruction::I32Const(0));
            wasm_fn.instruction(&Instruction::I32Ne);
        }

        _ => {
            return Err(format!(
                "WASM codegen: unsupported cast {src_kind:?} -> {dst_kind:?} ({mode:?})"
            ));
        }
    }
    Ok(())
}

/// Truncate fixed-point Q32.16 (`i32`) toward zero to the integer unit (divide by 65536).
fn emit_q32_fixed_trunc_toward_zero_to_i32(wasm_fn: &mut WasmFunction) {
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Const(65_536));
    wasm_fn.instruction(&Instruction::I64DivS);
    wasm_fn.instruction(&Instruction::I32WrapI64);
}

/// Clamp signed integer to `[-32768, 32767]` (representable Q16.16 **integer** units).
fn emit_clamp_i32_to_q16_int_range(wasm_fn: &mut WasmFunction, scratch: u32) {
    wasm_fn.instruction(&Instruction::LocalSet(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::I32Const(32_767));
    wasm_fn.instruction(&Instruction::I32GtS);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(32_767));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::End);
    wasm_fn.instruction(&Instruction::LocalSet(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::I32Const(-32_768));
    wasm_fn.instruction(&Instruction::I32LtS);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(-32_768));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::End);
}

/// Clamp signed i32 to Q16.16 **integer** range then scale to fixed (`× 65536`).
fn emit_i32_clamp_then_q32_scale_sint(
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let scratch = alloc
        .q32_scratch
        .map(|(a, _)| a)
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    emit_clamp_i32_to_q16_int_range(wasm_fn, scratch);
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Const(16));
    wasm_fn.instruction(&Instruction::I64Shl);
    wasm_fn.instruction(&Instruction::I32WrapI64);
    Ok(())
}

/// Clamp unsigned i32 to `0..=32767` then scale to fixed.
fn emit_u32_clamp_then_q32_scale_uint(
    wasm_fn: &mut WasmFunction,
    alloc: &LocalAlloc,
) -> Result<(), String> {
    let scratch = alloc
        .q32_scratch
        .map(|(a, _)| a)
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::I32Const(32_767));
    wasm_fn.instruction(&Instruction::I32GtU);
    wasm_fn.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
    wasm_fn.instruction(&Instruction::I32Const(32_767));
    wasm_fn.instruction(&Instruction::Else);
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::End);
    wasm_fn.instruction(&Instruction::LocalSet(scratch));
    wasm_fn.instruction(&Instruction::LocalGet(scratch));
    wasm_fn.instruction(&Instruction::I64ExtendI32U);
    wasm_fn.instruction(&Instruction::I64Const(16));
    wasm_fn.instruction(&Instruction::I64Shl);
    wasm_fn.instruction(&Instruction::I32WrapI64);
    Ok(())
}

fn emit_q32_mul(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    let (s0, s1) = alloc
        .q32_scratch
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Mul);
    wasm_fn.instruction(&Instruction::I64Const(16));
    wasm_fn.instruction(&Instruction::I64ShrS);
    wasm_fn.instruction(&Instruction::I32WrapI64);
    Ok(())
}

fn emit_q32_div(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    let (s0, s1) = alloc
        .q32_scratch
        .ok_or_else(|| String::from("Q32 scratch missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64Const(16));
    wasm_fn.instruction(&Instruction::I64Shl);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::I64ExtendI32S);
    wasm_fn.instruction(&Instruction::I64DivS);
    wasm_fn.instruction(&Instruction::I32WrapI64);
    Ok(())
}

/// `a - trunc(a/b)*b` using two f32 scratch locals.
fn emit_f32_mod(wasm_fn: &mut WasmFunction, alloc: &LocalAlloc) -> Result<(), String> {
    let (s0, s1) = alloc
        .float_scratch
        .ok_or_else(|| String::from("float scratch missing"))?;
    wasm_fn.instruction(&Instruction::LocalSet(s1));
    wasm_fn.instruction(&Instruction::LocalSet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::F32Div);
    wasm_fn.instruction(&Instruction::F32Trunc);
    wasm_fn.instruction(&Instruction::LocalGet(s1));
    wasm_fn.instruction(&Instruction::F32Mul);
    wasm_fn.instruction(&Instruction::LocalGet(s0));
    wasm_fn.instruction(&Instruction::F32Sub);
    Ok(())
}
