//! `GlslExecutable` — uniform interface for running compiled GLSL from filetests.

use alloc::{string::String, vec::Vec};

use lp_glsl_diagnostics::GlslError;
use lps_types::{LpsFnSig, LpsType};
use lpvm::LpsValue;

pub trait GlslExecutable {
    fn call_void(&mut self, name: &str, args: &[LpsValue]) -> Result<(), GlslError>;

    fn call_i32(&mut self, name: &str, args: &[LpsValue]) -> Result<i32, GlslError>;

    fn call_f32(&mut self, name: &str, args: &[LpsValue]) -> Result<f32, GlslError>;

    fn call_bool(&mut self, name: &str, args: &[LpsValue]) -> Result<bool, GlslError>;

    fn call_bvec(
        &mut self,
        name: &str,
        args: &[LpsValue],
        dim: usize,
    ) -> Result<Vec<bool>, GlslError>;

    fn call_ivec(
        &mut self,
        name: &str,
        args: &[LpsValue],
        dim: usize,
    ) -> Result<Vec<i32>, GlslError>;

    fn call_uvec(
        &mut self,
        name: &str,
        args: &[LpsValue],
        dim: usize,
    ) -> Result<Vec<u32>, GlslError>;

    fn call_vec(
        &mut self,
        name: &str,
        args: &[LpsValue],
        dim: usize,
    ) -> Result<Vec<f32>, GlslError>;

    fn call_mat(
        &mut self,
        name: &str,
        args: &[LpsValue],
        rows: usize,
        cols: usize,
    ) -> Result<Vec<f32>, GlslError>;

    /// Call a function that returns a fixed-size array; one [`LpsValue`] per array element.
    fn call_array(
        &mut self,
        name: &str,
        args: &[LpsValue],
        elem_ty: &LpsType,
        len: usize,
    ) -> Result<Vec<LpsValue>, GlslError>;

    fn get_function_signature(&self, name: &str) -> Option<&LpsFnSig>;

    fn list_functions(&self) -> Vec<String>;

    #[cfg(feature = "std")]
    fn format_emulator_state(&self) -> Option<String> {
        None
    }

    #[cfg(feature = "std")]
    fn format_clif_ir(&self) -> (Option<String>, Option<String>) {
        (None, None)
    }

    #[cfg(feature = "std")]
    fn format_vcode(&self) -> Option<String> {
        None
    }

    #[cfg(feature = "std")]
    fn format_disassembly(&self) -> Option<String> {
        None
    }
}
