//! Helper functions for built-in function code generation

use crate::error::GlslError;
use crate::frontend::codegen::context::CodegenContext;
use cranelift_codegen::ir::{AbiParam, FuncRef, Signature, types};
use cranelift_codegen::isa::CallConv;

impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    /// Helper to declare and get FuncRef for external math library function
    ///
    /// Creates external function calls using TestCase names (e.g., "sinf", "cosf").
    /// These are converted to q32 builtins by the transform.
    pub fn get_math_libcall(&mut self, func_name: &str) -> Result<FuncRef, GlslError> {
        // Create signature: f32 -> f32
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::F32));
        sig.returns.push(AbiParam::new(types::F32));

        // Create TestCase name for external function call
        let sig_ref = self.builder.func.import_signature(sig);
        let ext_name = cranelift_codegen::ir::ExternalName::testcase(func_name.as_bytes());
        let ext_func = cranelift_codegen::ir::ExtFuncData {
            name: ext_name,
            signature: sig_ref,
            colocated: false,
        };
        Ok(self.builder.func.import_function(ext_func))
    }

    /// Helper to declare and get FuncRef for 2-arg math function
    ///
    /// Creates external function calls using TestCase names (e.g., "atan2f", "powf").
    /// These are converted to q32 builtins by the transform.
    pub fn get_math_libcall_2arg(&mut self, func_name: &str) -> Result<FuncRef, GlslError> {
        // Create signature: (f32, f32) -> f32
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::F32));
        sig.params.push(AbiParam::new(types::F32));
        sig.returns.push(AbiParam::new(types::F32));

        // Create TestCase name for external function call
        let sig_ref = self.builder.func.import_signature(sig);
        let ext_name = cranelift_codegen::ir::ExternalName::testcase(func_name.as_bytes());
        let ext_func = cranelift_codegen::ir::ExtFuncData {
            name: ext_name,
            signature: sig_ref,
            colocated: false,
        };
        Ok(self.builder.func.import_function(ext_func))
    }

    /// Helper to declare and get FuncRef for atan2 (2-arg function)
    ///
    /// Creates external function calls using TestCase name "atan2f".
    /// This is converted to q32 builtins by the transform.
    pub fn get_atan2_libcall(&mut self) -> Result<FuncRef, GlslError> {
        self.get_math_libcall_2arg("atan2f")
    }
}
