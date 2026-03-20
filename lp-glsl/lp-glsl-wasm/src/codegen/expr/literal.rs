//! Literal expression code generation.

use wasm_encoder::InstructionSink;

use crate::codegen::numeric::WasmNumericMode;
use lp_glsl_frontend::error::GlslDiagnostics;
use lp_glsl_frontend::semantic::const_eval::ConstValue;
use lp_glsl_frontend::semantic::types::Type;

/// Q16.16 scale factor for float literals
const Q16_16_SCALE: f32 = 65536.0;

/// Type of a literal expression.
pub fn literal_type(expr: &glsl::syntax::Expr) -> Type {
    match expr {
        glsl::syntax::Expr::IntConst(_, _) => Type::Int,
        glsl::syntax::Expr::UIntConst(_, _) => Type::UInt,
        glsl::syntax::Expr::FloatConst(_, _) => Type::Float,
        glsl::syntax::Expr::BoolConst(_, _) => Type::Bool,
        _ => Type::Error,
    }
}

/// Emit literal (int, float, bool) onto stack. Returns the literal's type.
pub fn emit_literal(
    sink: &mut InstructionSink,
    expr: &glsl::syntax::Expr,
    numeric: WasmNumericMode,
) -> Result<Type, GlslDiagnostics> {
    let ty = literal_type(expr);
    match expr {
        glsl::syntax::Expr::IntConst(n, _) => {
            sink.i32_const(*n);
        }
        glsl::syntax::Expr::UIntConst(n, _) => {
            sink.i32_const(*n as i32);
        }
        glsl::syntax::Expr::FloatConst(f, _) => match numeric {
            WasmNumericMode::Q32 => {
                let fixed = (f * Q16_16_SCALE).round() as i32;
                sink.i32_const(fixed);
            }
            WasmNumericMode::Float => {
                sink.f32_const((*f).into());
            }
        },
        glsl::syntax::Expr::BoolConst(b, _) => {
            sink.i32_const(if *b { 1 } else { 0 });
        }
        _ => {
            return Err(lp_glsl_frontend::error::GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!("expected literal, got {:?}", expr),
            )
            .into());
        }
    }
    Ok(ty)
}

fn emit_float_component(sink: &mut InstructionSink, f: f32, numeric: WasmNumericMode) {
    match numeric {
        WasmNumericMode::Q32 => {
            let fixed = (f * Q16_16_SCALE).round() as i32;
            sink.i32_const(fixed);
        }
        WasmNumericMode::Float => {
            sink.f32_const(f.into());
        }
    }
}

/// Emit a compile-time `ConstValue` (module `const` or folded constant) onto the stack.
pub fn emit_const_value(
    sink: &mut InstructionSink,
    val: &ConstValue,
    numeric: WasmNumericMode,
) -> Result<Type, GlslDiagnostics> {
    let ty = val.glsl_type();
    match val {
        ConstValue::Int(n) => {
            sink.i32_const(*n);
        }
        ConstValue::UInt(n) => {
            sink.i32_const(*n as i32);
        }
        ConstValue::Float(f) => {
            emit_float_component(sink, *f, numeric);
        }
        ConstValue::Bool(b) => {
            sink.i32_const(if *b { 1 } else { 0 });
        }
        ConstValue::Vec2(v) => {
            for &c in v {
                emit_float_component(sink, c, numeric);
            }
        }
        ConstValue::Vec3(v) => {
            for &c in v {
                emit_float_component(sink, c, numeric);
            }
        }
        ConstValue::Vec4(v) => {
            for &c in v {
                emit_float_component(sink, c, numeric);
            }
        }
        ConstValue::IVec2(v) => {
            sink.i32_const(v[0]);
            sink.i32_const(v[1]);
        }
        ConstValue::UVec2(v) => {
            sink.i32_const(v[0] as i32);
            sink.i32_const(v[1] as i32);
        }
        ConstValue::IVec3(v) => {
            sink.i32_const(v[0]);
            sink.i32_const(v[1]);
            sink.i32_const(v[2]);
        }
        ConstValue::UVec3(v) => {
            sink.i32_const(v[0] as i32);
            sink.i32_const(v[1] as i32);
            sink.i32_const(v[2] as i32);
        }
        ConstValue::IVec4(v) => {
            sink.i32_const(v[0]);
            sink.i32_const(v[1]);
            sink.i32_const(v[2]);
            sink.i32_const(v[3]);
        }
        ConstValue::UVec4(v) => {
            sink.i32_const(v[0] as i32);
            sink.i32_const(v[1] as i32);
            sink.i32_const(v[2] as i32);
            sink.i32_const(v[3] as i32);
        }
        ConstValue::BVec2(v) => {
            sink.i32_const(if v[0] { 1 } else { 0 });
            sink.i32_const(if v[1] { 1 } else { 0 });
        }
        ConstValue::BVec3(v) => {
            sink.i32_const(if v[0] { 1 } else { 0 });
            sink.i32_const(if v[1] { 1 } else { 0 });
            sink.i32_const(if v[2] { 1 } else { 0 });
        }
        ConstValue::BVec4(v) => {
            sink.i32_const(if v[0] { 1 } else { 0 });
            sink.i32_const(if v[1] { 1 } else { 0 });
            sink.i32_const(if v[2] { 1 } else { 0 });
            sink.i32_const(if v[3] { 1 } else { 0 });
        }
        ConstValue::Mat2(m) => {
            emit_float_component(sink, m[0][0], numeric);
            emit_float_component(sink, m[0][1], numeric);
            emit_float_component(sink, m[1][0], numeric);
            emit_float_component(sink, m[1][1], numeric);
        }
    }
    Ok(ty)
}
