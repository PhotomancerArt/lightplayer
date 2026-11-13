//! An [`ImportHandler`] for the `@glsl::*` and `@lpir::*` imports, in native
//! f32 through `libm`: what the interpreter tiers (the `interp.f32` filetest
//! target, the conformance oracle, the shader agent's probe and the GPU
//! parity suites' oracle) call transcendental imports with.

use alloc::format;
use alloc::vec::Vec;

use crate::interp::{ImportHandler, InterpError, Value};

/// Dispatches `@glsl` and `@lpir` imports using `libm` (`f32` paths).
#[derive(Debug, Default)]
pub struct StdMathHandler;

impl ImportHandler for StdMathHandler {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, InterpError> {
        let f = |i: usize| {
            args.get(i)
                .and_then(|v| v.as_f32())
                .ok_or_else(|| InterpError::Import(format!("bad f32 arg {i}")))
        };
        let i = |i: usize| {
            args.get(i)
                .and_then(|v| v.as_i32())
                .ok_or_else(|| InterpError::Import(format!("bad i32 arg {i}")))
        };
        let out = match module_name {
            "glsl" => match func_name {
                "sin" => libm::sinf(f(0)?),
                "cos" => libm::cosf(f(0)?),
                "tan" => libm::tanf(f(0)?),
                "asin" => libm::asinf(f(0)?),
                "acos" => libm::acosf(f(0)?),
                "atan" => libm::atanf(f(0)?),
                "atan2" => libm::atan2f(f(0)?, f(1)?),
                "sinh" => libm::sinhf(f(0)?),
                "cosh" => libm::coshf(f(0)?),
                "tanh" => libm::tanhf(f(0)?),
                "asinh" => libm::asinhf(f(0)?),
                "acosh" => libm::acoshf(f(0)?),
                "atanh" => libm::atanhf(f(0)?),
                "exp" => libm::expf(f(0)?),
                "exp2" => libm::exp2f(f(0)?),
                "log" => libm::logf(f(0)?),
                "log2" => libm::log2f(f(0)?),
                "pow" => libm::powf(f(0)?, f(1)?),
                "ldexp" => libm::ldexpf(f(0)?, i(1)?),
                "round" => libm::roundf(f(0)?),
                _ => {
                    return Err(InterpError::Import(format!(
                        "unknown glsl function {func_name}"
                    )));
                }
            },
            "lpir" => match func_name {
                "sqrt" => libm::sqrtf(f(0)?),
                _ => {
                    return Err(InterpError::Import(format!(
                        "unknown lpir function {func_name}"
                    )));
                }
            },
            _ => {
                return Err(InterpError::Import(format!("unknown module {module_name}")));
            }
        };
        Ok(alloc::vec![Value::F32(out)])
    }
}
