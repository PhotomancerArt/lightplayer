//! LValue abstraction for unified handling of modifiable locations
//!
//! This module provides a unified interface for handling all modifiable locations
//! (variables, vector components, matrix elements, etc.) in a single place,
//! eliminating code duplication across assignment, increment, and decrement operations.

mod read;
mod resolve;
mod types;
mod utils;
mod write;

pub use read::read_lvalue;
pub use resolve::resolve_lvalue;
pub use types::{LValue, PointerAccessPattern};
pub use write::write_lvalue;

use crate::frontend::codegen::context::CodegenContext;
use crate::frontend::codegen::rvalue::RValue;
use glsl::syntax::Expr;

/// Common pattern: resolve expression as LValue, then load it as RValue
///
/// This pattern is used for Variable, Dot, and Bracket expressions.
/// First resolves the expression to a modifiable location (LValue),
/// then reads the current value(s) from that location.
pub fn emit_lvalue_as_rvalue<M: cranelift_module::Module>(
    ctx: &mut CodegenContext<'_, M>,
    expr: &Expr,
) -> Result<RValue, crate::error::GlslError> {
    let lvalue = resolve_lvalue(ctx, expr)?;
    let (vals, ty) = read_lvalue(ctx, &lvalue)?;
    Ok(RValue::from_aggregate(vals, ty))
}
