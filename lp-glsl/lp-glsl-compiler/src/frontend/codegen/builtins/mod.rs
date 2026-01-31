//! Code generation for GLSL built-in functions

mod common;
mod geometric;
mod helpers;
mod interpolation;
mod matrix;
mod relational;
mod trigonometric;

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type;
use cranelift_codegen::ir::Value;

use alloc::{format, vec::Vec};
impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    pub fn emit_builtin_call(
        &mut self,
        name: &str,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        match name {
            // Geometric Functions
            "dot" => self.builtin_dot(args),
            "cross" => self.builtin_cross(args),
            "length" => self.builtin_length(args),
            "normalize" => self.builtin_normalize(args),
            "distance" => self.builtin_distance(args),

            // Common Functions
            "min" => self.builtin_min(args),
            "max" => self.builtin_max(args),
            "clamp" => self.builtin_clamp(args),
            "abs" => self.builtin_abs(args),
            "sqrt" => self.builtin_sqrt(args),
            "inversesqrt" => self.builtin_inversesqrt(args),
            "floor" => self.builtin_floor(args),
            "ceil" => self.builtin_ceil(args),
            "round" => self.builtin_round(args),
            "roundEven" => self.builtin_roundeven(args),
            "fract" => self.builtin_fract(args),
            "mod" => self.builtin_mod(args),
            "sign" => self.builtin_sign(args),
            "pow" => self.builtin_pow(args),
            "exp" => self.builtin_exp(args),
            "log" => self.builtin_log(args),
            "exp2" => self.builtin_exp2(args),
            "log2" => self.builtin_log2(args),
            "isinf" => self.builtin_isinf(args),
            "isnan" => self.builtin_isnan(args),

            // Angle and Trigonometry Functions
            "radians" => self.builtin_radians(args),
            "degrees" => self.builtin_degrees(args),
            "sin" => self.builtin_sin(args),
            "cos" => self.builtin_cos(args),
            "tan" => self.builtin_tan(args),
            "asin" => self.builtin_asin(args),
            "acos" => self.builtin_acos(args),
            "atan" => self.builtin_atan(args),
            "sinh" => self.builtin_sinh(args),
            "cosh" => self.builtin_cosh(args),
            "tanh" => self.builtin_tanh(args),
            "asinh" => self.builtin_asinh(args),
            "acosh" => self.builtin_acosh(args),
            "atanh" => self.builtin_atanh(args),

            // Interpolation Functions
            "mix" => self.builtin_mix(args),
            "step" => self.builtin_step(args),
            "smoothstep" => self.builtin_smoothstep(args),

            // Relational Functions
            "all" => self.builtin_all(args),
            "any" => self.builtin_any(args),
            "not" => self.builtin_not(args),
            "equal" => self.builtin_equal(args),
            "notEqual" => self.builtin_not_equal(args),
            "greaterThan" => self.builtin_greater_than(args),
            "greaterThanEqual" => self.builtin_greater_than_equal(args),
            "lessThan" => self.builtin_less_than(args),
            "lessThanEqual" => self.builtin_less_than_equal(args),

            // Matrix Functions
            "matrixCompMult" => self.builtin_matrixCompMult(args),
            "outerProduct" => self.builtin_outerProduct(args),
            "transpose" => self.builtin_transpose(args),
            "determinant" => self.builtin_determinant(args),
            "inverse" => self.builtin_inverse(args),

            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("built-in function not implemented: {name}"),
            )),
        }
    }
}
