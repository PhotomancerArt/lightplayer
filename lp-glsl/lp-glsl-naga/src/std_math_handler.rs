//! Test [`lpir::ImportHandler`] for `@std.math::*` (phase 5+).

use alloc::format;
use alloc::vec::Vec;

use lpir::{ImportHandler, InterpError, Value};

/// Placeholder handler; real math dispatch is filled in with phase 5.
pub struct StdMathHandler;

impl ImportHandler for StdMathHandler {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        _args: &[Value],
    ) -> Result<Vec<Value>, InterpError> {
        Err(InterpError::Import(format!(
            "std.math handler not implemented: @{module_name}::{func_name}"
        )))
    }
}
