//! Expression code generation
//!
//! This module handles translation of GLSL expressions to Cranelift IR.
//! Each expression type has its own module:
//! - `literal`: Integer, float, and boolean literals
//! - `variable`: Variable references
//! - `binary`: Binary operations (+, -, *, /, ==, !=, etc.)
//! - `unary`: Unary operations (-, !)
//! - `function`: Function calls (built-in and user-defined)
//! - `constructor`: Type constructors (vec3(), mat2(), etc.)
//! - `vector`: Vector operations
//! - `matrix`: Matrix operations
//! - `component`: Component access and swizzling
//! - `coercion`: Type coercion and implicit conversions

pub mod assignment;
pub mod binary;
pub mod coercion;
pub mod component;
pub mod constructor;
pub mod function;
pub mod incdec;
pub mod literal;
pub mod matrix;
pub mod ternary;
pub mod unary;
pub mod variable;
pub mod vector;

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::rvalue::RValue;
use crate::semantic::types::Type as GlslType;
use cranelift_codegen::ir::Value;
use glsl::syntax::Expr;

use alloc::{format, vec::Vec};

impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    /// Emit code to compute an RValue (right-hand value)
    ///
    /// This is the new primary entry point for expression evaluation,
    /// following Clang's pattern of separating LValue (locations) from RValue (values).
    pub fn emit_rvalue(&mut self, expr: &Expr) -> Result<RValue, GlslError> {
        // Ensure we're in a block before evaluating expressions
        self.ensure_block()?;
        self.emit_rvalue_impl(expr)
    }

    /// Internal dispatch - clean match statement delegating to helpers
    fn emit_rvalue_impl(&mut self, expr: &Expr) -> Result<RValue, GlslError> {
        match expr {
            // Simple delegations stay inline
            Expr::IntConst(..)
            | Expr::UIntConst(..)
            | Expr::FloatConst(..)
            | Expr::BoolConst(..) => literal::emit_literal_rvalue(self, expr),
            Expr::Binary(..) => binary::emit_binary_rvalue(self, expr),
            Expr::FunCall(..) => function::emit_function_call_rvalue(self, expr),

            // Complex cases delegate to helpers
            Expr::Variable(..) => variable::emit_variable_rvalue(self, expr),
            Expr::Unary(..) => unary::emit_unary_rvalue(self, expr),
            Expr::Dot(..) => component::emit_component_access_rvalue(self, expr),
            Expr::Bracket(..) => component::emit_indexing_rvalue(self, expr),
            Expr::Assignment(..) => assignment::emit_assignment_rvalue(self, expr),
            Expr::PostInc(..) => incdec::emit_postinc_rvalue(self, expr),
            Expr::PostDec(..) => incdec::emit_postdec_rvalue(self, expr),
            Expr::Ternary(..) => ternary::emit_ternary_rvalue(self, expr),

            _ => Err(GlslError::new(
                ErrorCode::E0400,
                format!("expression not supported yet: {expr:?}"),
            )),
        }
    }

    /// Emit code to compute an LValue (left-hand value - modifiable location)
    ///
    /// This resolves an expression to a modifiable location, following Clang's pattern.
    pub fn emit_lvalue(
        &mut self,
        expr: &Expr,
    ) -> Result<crate::frontend::codegen::lvalue::LValue, GlslError> {
        use crate::frontend::codegen::lvalue::resolve_lvalue;
        resolve_lvalue(self, expr)
    }

    /// Load an LValue to get its RValue
    ///
    /// This reads the current value(s) from a modifiable location.
    pub fn load_lvalue(
        &mut self,
        lvalue: crate::frontend::codegen::lvalue::LValue,
    ) -> Result<RValue, GlslError> {
        use crate::frontend::codegen::lvalue::read_lvalue;
        let (vals, ty) = read_lvalue(self, &lvalue)?;
        Ok(RValue::from_aggregate(vals, ty))
    }

    /// Main entry point for expression translation (legacy - use emit_rvalue instead)
    ///
    /// This method is kept for backwards compatibility during the transition.
    /// New code should use `emit_rvalue` instead.
    pub fn emit_expr_typed(&mut self, expr: &Expr) -> Result<(Vec<Value>, GlslType), GlslError> {
        let rvalue = self.emit_rvalue(expr)?;
        let ty = rvalue.ty().clone();
        Ok((rvalue.into_values(), ty))
    }

    /// Legacy wrapper for compatibility - returns just the first value (for scalars)
    pub fn emit_expr(&mut self, expr: &Expr) -> Result<Value, GlslError> {
        let (vals, _ty) = self.emit_expr_typed(expr)?;
        let expr_span = crate::error::extract_span_from_expr(expr);
        vals.into_iter().next().ok_or_else(|| {
            let error = GlslError::new(ErrorCode::E0400, "expression produced no values")
                .with_location(crate::error::source_span_to_location(&expr_span));
            self.add_span_to_error(error, &expr_span)
        })
    }

    /// Coerce a value from one type to another (implements GLSL implicit conversions)
    pub fn coerce_to_type(
        &mut self,
        val: cranelift_codegen::ir::Value,
        from_ty: &GlslType,
        to_ty: &GlslType,
    ) -> Result<cranelift_codegen::ir::Value, GlslError> {
        coercion::coerce_to_type(self, val, from_ty, to_ty)
    }

    pub fn coerce_to_type_with_location(
        &mut self,
        val: cranelift_codegen::ir::Value,
        from_ty: &GlslType,
        to_ty: &GlslType,
        span: Option<glsl::syntax::SourceSpan>,
    ) -> Result<cranelift_codegen::ir::Value, GlslError> {
        coercion::coerce_to_type_with_location(self, val, from_ty, to_ty, span)
    }
}
