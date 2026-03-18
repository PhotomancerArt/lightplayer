//! Literal expression code generation.

use wasm_encoder::InstructionSink;

use crate::codegen::numeric::WasmNumericMode;
use lp_glsl_frontend::error::GlslDiagnostics;
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
