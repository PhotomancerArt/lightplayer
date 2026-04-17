//! LightPlayer intermediate representation (LPIR) — flat, scalarized IR with structured control flow.
//!
//! See `docs/lpir/` for the language specification.

#![no_std]

extern crate alloc;

pub mod builder;
pub mod compiler_config;
pub mod const_fold;
mod inline;
pub mod interp;
pub mod lpir_module;
pub mod lpir_op;
pub mod parse;
pub mod print;
pub mod types;
pub mod validate;

#[cfg(test)]
mod tests;

pub use builder::{FunctionBuilder, ModuleBuilder};
pub use compiler_config::{CompilerConfig, ConfigError, InlineConfig, InlineMode};
pub use inline::{inline_module, InlineResult};
pub use interp::{ImportHandler, InterpError, Value, interpret, interpret_with_depth};
pub use lpir_module::{ImportDecl, IrFunction, LpirModule, SlotDecl, VMCTX_VREG};
pub use lpir_op::LpirOp;
pub use parse::{ParseError, parse_module};
pub use print::print_module;
pub use types::{CalleeRef, FloatMode, FuncId, ImportId, IrType, SlotId, VReg, VRegRange};
pub use validate::{ValidationError, validate_function, validate_module};

/// Candidate inline size metrics for M3.1 (`func_weight` tuning). See [`inline_weights`].
pub mod inline_weights {
    pub use crate::inline::heuristic::{
        weight, weight_body_len, weight_heavy_bias, weight_markers_zero, WeightKind,
    };
}
