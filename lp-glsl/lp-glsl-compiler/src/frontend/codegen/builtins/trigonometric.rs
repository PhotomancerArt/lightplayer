//! Angle and trigonometry built-in functions

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type;
use cranelift_codegen::ir::{InstBuilder, Value};

use alloc::vec::Vec;

impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    /// radians(degrees) - Convert degrees to radians
    pub fn builtin_radians(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (deg_vals, deg_ty) = &args[0];
        let pi_over_180 = self.builder.ins().f32const(0.017453292519943295); // π/180
        let mut result_vals = Vec::new();

        for &deg in deg_vals {
            result_vals.push(self.builder.ins().fmul(deg, pi_over_180));
        }

        Ok((result_vals, deg_ty.clone()))
    }

    /// degrees(radians) - Convert radians to degrees
    pub fn builtin_degrees(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (rad_vals, rad_ty) = &args[0];
        let _180_over_pi = self.builder.ins().f32const(57.29577951308232); // 180/π
        let mut result_vals = Vec::new();

        for &rad in rad_vals {
            result_vals.push(self.builder.ins().fmul(rad, _180_over_pi));
        }

        Ok((result_vals, rad_ty.clone()))
    }

    /// sin(angle) - Sine (component-wise)
    pub fn builtin_sin(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("sinf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// cos(angle) - Cosine (component-wise)
    pub fn builtin_cos(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("cosf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// tan(angle) - Tangent (component-wise)
    pub fn builtin_tan(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("tanf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// asin(x) - Arc sine (component-wise)
    pub fn builtin_asin(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("asinf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// acos(x) - Arc cosine (component-wise)
    pub fn builtin_acos(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("acosf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// atan(x) or atan(y, x) - Arc tangent (component-wise)
    ///
    /// GLSL spec: atan(genFType y, genFType x) - first arg is y, second is x
    pub fn builtin_atan(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (y_over_x_vals, ty) = &args[0];

        if args.len() == 1 {
            // 1-arg version: atan(y_over_x)
            let func_ref = self.get_math_libcall("atanf")?;
            let mut result_vals = Vec::new();

            for &val in y_over_x_vals {
                let call_inst = self.builder.ins().call(func_ref, &[val]);
                result_vals.push(self.builder.inst_results(call_inst)[0]);
            }

            Ok((result_vals, ty.clone()))
        } else {
            // 2-arg version: atan(y, x) - GLSL spec: first arg is y, second is x
            let (y_vals, y_ty) = &args[0]; // First argument is y
            let (x_vals, _) = &args[1]; // Second argument is x

            if y_vals.len() != x_vals.len() {
                return Err(GlslError::new(
                    ErrorCode::E0104,
                    "atan() 2-arg version requires matching vector sizes",
                ));
            }

            let func_ref = self.get_atan2_libcall()?;
            let mut result_vals = Vec::new();

            for i in 0..y_vals.len() {
                // Call atan2f(y, x) - correct order per GLSL spec
                let call_inst = self.builder.ins().call(func_ref, &[y_vals[i], x_vals[i]]);
                result_vals.push(self.builder.inst_results(call_inst)[0]);
            }

            Ok((result_vals, y_ty.clone()))
        }
    }

    /// sinh(x) - Hyperbolic sine (component-wise)
    pub fn builtin_sinh(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("sinhf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// cosh(x) - Hyperbolic cosine (component-wise)
    pub fn builtin_cosh(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("coshf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// tanh(x) - Hyperbolic tangent (component-wise)
    pub fn builtin_tanh(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("tanhf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// asinh(x) - Inverse hyperbolic sine (component-wise)
    pub fn builtin_asinh(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("asinhf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// acosh(x) - Inverse hyperbolic cosine (component-wise)
    pub fn builtin_acosh(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("acoshf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// atanh(x) - Inverse hyperbolic tangent (component-wise)
    pub fn builtin_atanh(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let func_ref = self.get_math_libcall("atanhf")?;
        let mut result_vals = Vec::new();

        for &val in x_vals {
            let call_inst = self.builder.ins().call(func_ref, &[val]);
            result_vals.push(self.builder.inst_results(call_inst)[0]);
        }

        Ok((result_vals, x_ty.clone()))
    }
}
