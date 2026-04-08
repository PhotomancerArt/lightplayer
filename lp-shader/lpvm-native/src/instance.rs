//! [`LpvmInstance`] — per-instance execution state (M3).

use alloc::string::String;
use alloc::vec::Vec;

use lps_shared::lps_value_f32::LpsValueF32;
use lpvm::LpvmInstance;

use crate::error::NativeError;

/// Execution instance placeholder.
#[derive(Debug, Default)]
pub struct NativeInstance;

impl LpvmInstance for NativeInstance {
    type Error = NativeError;

    fn call(&mut self, _name: &str, _args: &[LpsValueF32]) -> Result<LpsValueF32, Self::Error> {
        Err(NativeError::NotYetImplemented(String::from("M3: call")))
    }

    fn call_q32(&mut self, _name: &str, _args: &[i32]) -> Result<Vec<i32>, Self::Error> {
        Err(NativeError::NotYetImplemented(String::from("M3: call_q32")))
    }
}
