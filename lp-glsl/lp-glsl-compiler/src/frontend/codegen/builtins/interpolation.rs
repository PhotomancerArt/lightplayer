//! Interpolation built-in functions

use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type;
use cranelift_codegen::ir::{InstBuilder, Value, condcodes::IntCC, types};

use alloc::vec::Vec;

impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    /// mix(x, y, a) = x * (1-a) + y * a (linear interpolation for floats)
    /// For boolean vectors: if selector is false, take from x; if true, take from y
    pub fn builtin_mix(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let (y_vals, _) = &args[1];
        let (a_vals, _a_ty) = &args[2];

        // Check if this is a boolean vector mix
        let base_ty = if x_ty.is_vector() {
            x_ty.vector_base_type().unwrap()
        } else {
            x_ty.clone()
        };

        if base_ty == Type::Bool {
            // Boolean vector mix: use selection logic
            return self.builtin_mix_bool(args);
        }

        let mut result_vals = Vec::new();

        // Handle scalar broadcast (mix(vec3, vec3, float))
        if x_vals.len() > 1 && a_vals.len() == 1 {
            let a_scalar = a_vals[0];
            // Compute (1 - a)
            let one = self.builder.ins().f32const(1.0);
            let one_minus_a = self.builder.ins().fsub(one, a_scalar);

            for i in 0..x_vals.len() {
                // x * (1-a)
                let x_part = self.builder.ins().fmul(x_vals[i], one_minus_a);
                // y * a
                let y_part = self.builder.ins().fmul(y_vals[i], a_scalar);
                // x * (1-a) + y * a
                result_vals.push(self.builder.ins().fadd(x_part, y_part));
            }
        } else {
            // Component-wise mix
            for i in 0..x_vals.len() {
                // (1 - a)
                let one = self.builder.ins().f32const(1.0);
                let one_minus_a = self.builder.ins().fsub(one, a_vals[i]);
                // x * (1-a)
                let x_part = self.builder.ins().fmul(x_vals[i], one_minus_a);
                // y * a
                let y_part = self.builder.ins().fmul(y_vals[i], a_vals[i]);
                // x * (1-a) + y * a
                result_vals.push(self.builder.ins().fadd(x_part, y_part));
            }
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// step(edge, x) = x < edge ? 0.0 : 1.0
    pub fn builtin_step(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (edge_vals, _) = &args[0];
        let (x_vals, x_ty) = &args[1];

        let mut result_vals = Vec::new();
        let zero = self.builder.ins().f32const(0.0);
        let one = self.builder.ins().f32const(1.0);

        // Handle scalar broadcast (step(float, vec3))
        if edge_vals.len() == 1 && x_vals.len() > 1 {
            let edge_scalar = edge_vals[0];
            for &x in x_vals {
                // x < edge ? 0.0 : 1.0
                let cmp = self.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::LessThan,
                    x,
                    edge_scalar,
                );
                result_vals.push(self.builder.ins().select(cmp, zero, one));
            }
        } else {
            // Component-wise step
            for i in 0..x_vals.len() {
                let cmp = self.builder.ins().fcmp(
                    cranelift_codegen::ir::condcodes::FloatCC::LessThan,
                    x_vals[i],
                    edge_vals[i],
                );
                result_vals.push(self.builder.ins().select(cmp, zero, one));
            }
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// smoothstep(edge0, edge1, x) - Smooth Hermite interpolation
    /// Formula: t = clamp((x - edge0) / (edge1 - edge0), 0, 1); return t * t * (3 - 2 * t);
    pub fn builtin_smoothstep(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (edge0_vals, _) = &args[0];
        let (edge1_vals, _) = &args[1];
        let (x_vals, x_ty) = &args[2];

        let mut result_vals = Vec::new();
        let zero = self.builder.ins().f32const(0.0);
        let one = self.builder.ins().f32const(1.0);
        let two = self.builder.ins().f32const(2.0);
        let three = self.builder.ins().f32const(3.0);

        // Handle scalar broadcast (smoothstep(float, float, vec3))
        if edge0_vals.len() == 1 && x_vals.len() > 1 {
            let edge0_scalar = edge0_vals[0];
            let edge1_scalar = edge1_vals[0];

            for &x in x_vals {
                // t = (x - edge0) / (edge1 - edge0)
                let numerator = self.builder.ins().fsub(x, edge0_scalar);
                let denominator = self.builder.ins().fsub(edge1_scalar, edge0_scalar);
                let t_raw = self.builder.ins().fdiv(numerator, denominator);

                // t = clamp(t, 0, 1)
                let t_max = self.builder.ins().fmax(t_raw, zero);
                let t_clamped = self.builder.ins().fmin(t_max, one);

                // result = t * t * (3 - 2 * t)
                let t_squared = self.builder.ins().fmul(t_clamped, t_clamped);
                let two_t = self.builder.ins().fmul(two, t_clamped);
                let three_minus_two_t = self.builder.ins().fsub(three, two_t);
                let result = self.builder.ins().fmul(t_squared, three_minus_two_t);

                result_vals.push(result);
            }
        } else {
            // Component-wise smoothstep
            for i in 0..x_vals.len() {
                // t = (x - edge0) / (edge1 - edge0)
                let numerator = self.builder.ins().fsub(x_vals[i], edge0_vals[i]);
                let denominator = self.builder.ins().fsub(edge1_vals[i], edge0_vals[i]);
                let t_raw = self.builder.ins().fdiv(numerator, denominator);

                // t = clamp(t, 0, 1)
                let t_max = self.builder.ins().fmax(t_raw, zero);
                let t_clamped = self.builder.ins().fmin(t_max, one);

                // result = t * t * (3 - 2 * t)
                let t_squared = self.builder.ins().fmul(t_clamped, t_clamped);
                let two_t = self.builder.ins().fmul(two, t_clamped);
                let three_minus_two_t = self.builder.ins().fsub(three, two_t);
                let result = self.builder.ins().fmul(t_squared, three_minus_two_t);

                result_vals.push(result);
            }
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// mix(x, y, a) - component-wise selection for boolean vectors
    /// For each component: if selector is false, take from x; if true, take from y
    fn builtin_mix_bool(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];
        let (y_vals, _) = &args[1];
        let (a_vals, _) = &args[2];

        let zero = self.builder.ins().iconst(types::I8, 0);

        let mut result_vals = Vec::new();
        for i in 0..x_vals.len() {
            // Check if selector is non-zero (true)
            let selector_true = self.builder.ins().icmp(IntCC::NotEqual, a_vals[i], zero);
            // Select: if selector is true, take y; else take x
            let result = self
                .builder
                .ins()
                .select(selector_true, y_vals[i], x_vals[i]);
            result_vals.push(result);
        }

        Ok((result_vals, x_ty.clone()))
    }
}
