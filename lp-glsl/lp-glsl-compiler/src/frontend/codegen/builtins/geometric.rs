//! Geometric built-in functions

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use crate::semantic::types::Type;
use cranelift_codegen::ir::{InstBuilder, Value};

use alloc::vec;
use alloc::vec::Vec;

impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    /// Dot product: x·y = x₀y₀ + x₁y₁ + x₂y₂ + ...
    pub fn builtin_dot(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, _) = &args[0];
        let (y_vals, _) = &args[1];

        if x_vals.len() != y_vals.len() {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "dot() requires matching vector sizes",
            ));
        }

        let mut sum = self.builder.ins().fmul(x_vals[0], y_vals[0]);
        for i in 1..x_vals.len() {
            let product = self.builder.ins().fmul(x_vals[i], y_vals[i]);
            sum = self.builder.ins().fadd(sum, product);
        }

        Ok((vec![sum], Type::Float))
    }

    /// Cross product: x × y (vec3 only)
    pub fn builtin_cross(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, _) = &args[0];
        let (y_vals, _) = &args[1];

        if x_vals.len() != 3 || y_vals.len() != 3 {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "cross() requires vec3 arguments",
            ));
        }

        // cross(x, y) = (x.y*y.z - x.z*y.y, x.z*y.x - x.x*y.z, x.x*y.y - x.y*y.x)
        let x0 = x_vals[0];
        let x1 = x_vals[1];
        let x2 = x_vals[2];
        let y0 = y_vals[0];
        let y1 = y_vals[1];
        let y2 = y_vals[2];

        let r0 = {
            let a = self.builder.ins().fmul(x1, y2);
            let b = self.builder.ins().fmul(x2, y1);
            self.builder.ins().fsub(a, b)
        };
        let r1 = {
            let a = self.builder.ins().fmul(x2, y0);
            let b = self.builder.ins().fmul(x0, y2);
            self.builder.ins().fsub(a, b)
        };
        let r2 = {
            let a = self.builder.ins().fmul(x0, y1);
            let b = self.builder.ins().fmul(x1, y0);
            self.builder.ins().fsub(a, b)
        };

        Ok((vec![r0, r1, r2], Type::Vec3))
    }

    /// length(x) = sqrt(dot(x, x))
    pub fn builtin_length(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, _) = &args[0];

        // Compute dot product with self
        let mut sum_sq = self.builder.ins().fmul(x_vals[0], x_vals[0]);
        for i in 1..x_vals.len() {
            let sq = self.builder.ins().fmul(x_vals[i], x_vals[i]);
            sum_sq = self.builder.ins().fadd(sum_sq, sq);
        }

        // Square root
        let result = self.builder.ins().sqrt(sum_sq);

        Ok((vec![result], Type::Float))
    }

    /// normalize(x) = x / length(x)
    pub fn builtin_normalize(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (x_vals, x_ty) = &args[0];

        // Compute length
        let (len_val, _) = self.builtin_length(vec![(x_vals.clone(), x_ty.clone())])?;
        let len = len_val[0];

        // Divide each component by length
        let mut result_vals = Vec::new();
        for &val in x_vals {
            result_vals.push(self.builder.ins().fdiv(val, len));
        }

        Ok((result_vals, x_ty.clone()))
    }

    /// distance(p0, p1) = length(p0 - p1)
    pub fn builtin_distance(
        &mut self,
        args: Vec<(Vec<Value>, Type)>,
    ) -> Result<(Vec<Value>, Type), GlslError> {
        let (p0_vals, p0_ty) = &args[0];
        let (p1_vals, _) = &args[1];

        if p0_vals.len() != p1_vals.len() {
            return Err(GlslError::new(
                ErrorCode::E0104,
                "distance() requires matching vector sizes",
            ));
        }

        // Compute p0 - p1
        let mut diff_vals = Vec::new();
        for i in 0..p0_vals.len() {
            diff_vals.push(self.builder.ins().fsub(p0_vals[i], p1_vals[i]));
        }

        // Compute length of difference
        self.builtin_length(vec![(diff_vals, p0_ty.clone())])
    }
}
