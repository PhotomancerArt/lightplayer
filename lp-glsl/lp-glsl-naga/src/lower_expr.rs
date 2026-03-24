//! Naga [`naga::Expression`] → LPIR ops (see phase 3).
#![allow(dead_code, reason = "Expression lowering implemented in phase 3.")]

use alloc::format;

use lpir::VReg;
use naga::Handle;

use crate::lower_ctx::LowerCtx;
use crate::lower_error::LowerError;

pub(crate) fn lower_expr(
    _ctx: &mut LowerCtx<'_>,
    expr: Handle<naga::Expression>,
) -> Result<VReg, LowerError> {
    Err(LowerError::UnsupportedExpression(format!(
        "expression lowering not implemented ({expr:?})"
    )))
}
