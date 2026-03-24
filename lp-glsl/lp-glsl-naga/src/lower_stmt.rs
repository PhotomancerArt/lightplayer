//! Naga statements / blocks → LPIR op stream (see phase 4).
#![allow(dead_code, reason = "Statement lowering implemented in phase 4.")]

use alloc::string::String;

use naga::Block;

use crate::lower_ctx::LowerCtx;
use crate::lower_error::LowerError;

pub(crate) fn lower_block(_ctx: &mut LowerCtx<'_>, _block: &Block) -> Result<(), LowerError> {
    Err(LowerError::UnsupportedStatement(String::from(
        "statement lowering not implemented",
    )))
}
