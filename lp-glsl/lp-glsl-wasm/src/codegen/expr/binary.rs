//! Binary expression code generation.

use wasm_encoder::InstructionSink;

use crate::codegen::numeric::WasmNumericMode;
use lp_glsl_frontend::error::GlslError;

/// Emit binary op. Phase ii: int/float scalar only.
/// For Q32, * uses builtin (phase iii); + and - use i32.add/sub.
pub fn emit_binary_op(
    sink: &mut InstructionSink,
    op: &glsl::syntax::BinaryOp,
    numeric: WasmNumericMode,
) -> Result<(), lp_glsl_frontend::error::GlslDiagnostics> {
    use glsl::syntax::BinaryOp::*;

    match op {
        Add => match numeric {
            WasmNumericMode::Q32 => {
                sink.i32_add();
            }
            WasmNumericMode::Float => {
                sink.f32_add();
            }
        },
        Sub => match numeric {
            WasmNumericMode::Q32 => {
                sink.i32_sub();
            }
            WasmNumericMode::Float => {
                sink.f32_sub();
            }
        },
        Mult => match numeric {
            WasmNumericMode::Q32 => {
                // Q32 mul needs builtin (shift). Phase ii: int only.
                return Err(GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0400,
                    "Q32 multiplication requires builtin (phase iii)",
                )
                .into());
            }
            WasmNumericMode::Float => {
                sink.f32_mul();
            }
        },
        Div => match numeric {
            WasmNumericMode::Q32 => {
                return Err(GlslError::new(
                    lp_glsl_frontend::error::ErrorCode::E0400,
                    "Q32 division requires builtin (phase iii)",
                )
                .into());
            }
            WasmNumericMode::Float => {
                sink.f32_div();
            }
        },
        Equal => {
            sink.i32_eq();
        }
        NonEqual => {
            sink.i32_ne();
        }
        LT => {
            sink.i32_lt_s();
        }
        GT => {
            sink.i32_gt_s();
        }
        LTE => {
            sink.i32_le_s();
        }
        GTE => {
            sink.i32_ge_s();
        }
        And | Or | Xor => {
            return Err(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                "logical op (phase iii)",
            )
            .into());
        }
        _ => {
            return Err(GlslError::new(
                lp_glsl_frontend::error::ErrorCode::E0400,
                alloc::format!("binary op {:?} not supported in phase ii", op),
            )
            .into());
        }
    };
    Ok(())
}
