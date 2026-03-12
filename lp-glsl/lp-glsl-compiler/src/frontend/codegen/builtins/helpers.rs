//! Helper functions for built-in function code generation

use crate::error::{ErrorCode, GlslError};
use crate::frontend::codegen::context::CodegenContext;
use cranelift_codegen::ir::{AbiParam, FuncRef, Signature, types};
use cranelift_codegen::isa::CallConv;

impl<'a, M: cranelift_module::Module> CodegenContext<'a, M> {
    fn get_q32_math_builtin(
        &mut self,
        func_name: &str,
        arg_count: usize,
    ) -> Result<FuncRef, GlslError> {
        use crate::backend::builtins::map_testcase_to_builtin;

        let builtin_id = map_testcase_to_builtin(func_name, arg_count).ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0400,
                alloc::format!("No Q32 builtin for '{func_name}'"),
            )
        })?;

        self.gl_module
            .get_builtin_func_ref(builtin_id, self.builder.func)
    }

    /// Helper to declare and get FuncRef for external math library function
    ///
    /// In Q32 mode, returns Q32 builtin FuncRef directly. In float mode, creates
    /// TestCase external calls (converted to q32 by transform when applicable).
    pub fn get_math_libcall(&mut self, func_name: &str) -> Result<FuncRef, GlslError> {
        if self.is_q32() {
            return self.get_q32_math_builtin(func_name, 1);
        }

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
    /// In Q32 mode, returns Q32 builtin FuncRef directly. In float mode, creates
    /// TestCase external calls.
    pub fn get_math_libcall_2arg(&mut self, func_name: &str) -> Result<FuncRef, GlslError> {
        if self.is_q32() {
            return self.get_q32_math_builtin(func_name, 2);
        }

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
    pub fn get_atan2_libcall(&mut self) -> Result<FuncRef, GlslError> {
        self.get_math_libcall_2arg("atan2f")
    }
}
