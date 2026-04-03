//! `GlslExecutable` — uniform interface for running compiled GLSL from filetests.

use alloc::{string::String, vec::Vec};

use lp_glsl_abi::GlslValue;
use lp_glsl_core::{FunctionSignature, Type};
use lp_glsl_diagnostics::GlslError;

pub trait GlslExecutable {
    fn call_void(&mut self, name: &str, args: &[GlslValue]) -> Result<(), GlslError>;

    fn call_i32(&mut self, name: &str, args: &[GlslValue]) -> Result<i32, GlslError>;

    fn call_f32(&mut self, name: &str, args: &[GlslValue]) -> Result<f32, GlslError>;

    fn call_bool(&mut self, name: &str, args: &[GlslValue]) -> Result<bool, GlslError>;

    fn call_bvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<bool>, GlslError>;

    fn call_ivec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<i32>, GlslError>;

    fn call_uvec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<u32>, GlslError>;

    fn call_vec(
        &mut self,
        name: &str,
        args: &[GlslValue],
        dim: usize,
    ) -> Result<Vec<f32>, GlslError>;

    fn call_mat(
        &mut self,
        name: &str,
        args: &[GlslValue],
        rows: usize,
        cols: usize,
    ) -> Result<Vec<f32>, GlslError>;

    /// Call a function that returns a fixed-size array; one [`GlslValue`] per array element.
    fn call_array(
        &mut self,
        name: &str,
        args: &[GlslValue],
        elem_ty: &Type,
        len: usize,
    ) -> Result<Vec<GlslValue>, GlslError>;

    fn get_function_signature(&self, name: &str) -> Option<&FunctionSignature>;

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
