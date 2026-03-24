//! LightPlayer intermediate representation (LPIR) — flat, scalarized IR with structured control flow.
//!
//! See `docs/lpir/` for the language specification.

#![no_std]

extern crate alloc;

pub mod builder;
pub mod interp;
pub mod module;
pub mod op;
pub mod parse;
pub mod print;
pub mod types;
pub mod validate;

#[cfg(test)]
mod tests;

pub use builder::{FunctionBuilder, ModuleBuilder};
pub use interp::{ImportHandler, InterpError, Value, interpret, interpret_with_depth};
pub use module::{ImportDecl, IrFunction, IrModule, SlotDecl};
pub use op::Op;
pub use parse::{ParseError, parse_module};
pub use print::print_module;
pub use types::{CalleeRef, IrType, SlotId, VReg, VRegRange};
pub use validate::{ValidationError, validate_function, validate_module};
