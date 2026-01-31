//! Test helper utilities for building functions programmatically
//!
//! These functions are only available in test builds and provide convenient
//! ways to build Functions for testing purposes.

#[cfg(test)]
pub mod test_helpers {
    use crate::backend::module::gl_func::GlFunc;
    use crate::backend::module::gl_module::GlModule;
    use crate::error::{ErrorCode, GlslError};
    use alloc::string::String;
    use alloc::vec::Vec;
    use cranelift_codegen::ir::{InstBuilder, Signature, Value};
    use cranelift_frontend::FunctionBuilder;
    use cranelift_frontend::FunctionBuilderContext;
    use cranelift_module::{FuncId, Linkage, Module};

    /// Build a simple function programmatically
    ///
    /// **Note**: Function must be declared before it can be called by other functions.
    /// Use `declare_function` first if you need to call this function from another.
    pub fn build_simple_function<M: Module>(
        gl_module: &mut GlModule<M>,
        name: &str,
        linkage: Linkage,
        sig: Signature,
        body: impl FnOnce(&mut FunctionBuilder) -> Result<(), GlslError>,
    ) -> Result<FuncId, GlslError> {
        // 1. Declare function in module first to get func_id
        let func_id = gl_module
            .module_mut_internal()
            .declare_function(name, linkage, &sig)
            .map_err(|e| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Failed to declare function '{name}': {e}"),
                )
            })?;

        // 2. Create context and builder
        let mut ctx = gl_module.module_internal().make_context();

        // 3. Set signature and name (before creating builder)
        ctx.func.signature = sig.clone();
        ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32());

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // 4. Build entry block
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // 5. Call user-provided body builder
        body(&mut builder)?;

        // 6. Finalize
        builder.finalize();

        // 7. Store Function IR (but don't define yet - that happens in build_executable)
        // Remove the placeholder if it exists
        gl_module.fns.remove(name);
        let function = ctx.func.clone(); // Clone before clearing context
        gl_module.fns.insert(
            String::from(name),
            GlFunc {
                name: String::from(name),
                clif_sig: sig,
                func_id,
                function,
            },
        );

        gl_module.module_internal().clear_context(&mut ctx);

        Ok(func_id)
    }

    /// Build a function that calls another function
    ///
    /// **Note**: The callee function must be declared before calling this function.
    /// The callee should be built using `build_simple_function` or `declare_function` first.
    pub fn build_call_function<M: Module>(
        gl_module: &mut GlModule<M>,
        name: &str,
        linkage: Linkage,
        sig: Signature,
        callee_name: &str,
        args_builder: impl FnOnce(&mut FunctionBuilder) -> Result<Vec<Value>, GlslError>,
    ) -> Result<FuncId, GlslError> {
        // Get callee FuncId before entering closure (to avoid borrowing issues)
        let callee_func_id = gl_module
            .get_func(callee_name)
            .ok_or_else(|| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Function '{callee_name}' not found (must be declared first)"),
                )
            })?
            .func_id;

        // Declare function first
        let func_id = gl_module
            .module_mut_internal()
            .declare_function(name, linkage, &sig)
            .map_err(|e| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Failed to declare function '{name}': {e}"),
                )
            })?;

        // Create context and set up function
        let mut ctx = gl_module.module_internal().make_context();
        ctx.func.signature = sig.clone();
        ctx.func.name = cranelift_codegen::ir::UserFuncName::user(0, func_id.as_u32());

        // Create FuncRef BEFORE creating builder (to avoid borrowing conflicts)
        let callee_ref = gl_module
            .module_mut_internal()
            .declare_func_in_func(callee_func_id, &mut ctx.func);

        let mut builder_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

        // Build entry block
        let entry_block = builder.create_block();
        builder.append_block_params_for_function_params(entry_block);
        builder.switch_to_block(entry_block);
        builder.seal_block(entry_block);

        // Build arguments using user-provided builder
        let args = args_builder(&mut builder)?;

        // Call the function
        let call_inst = builder.ins().call(callee_ref, &args);
        // Get results before using builder mutably again
        let result_values: Vec<Value> = builder.inst_results(call_inst).to_vec();

        if result_values.is_empty() {
            builder.ins().return_(&[]);
        } else {
            builder.ins().return_(&[result_values[0]]);
        }

        // Finalize
        builder.finalize();

        // Store Function IR (but don't define yet - that happens in build_executable)
        let function = ctx.func.clone(); // Clone before clearing context
        gl_module.fns.insert(
            String::from(name),
            GlFunc {
                name: String::from(name),
                clif_sig: sig,
                func_id,
                function,
            },
        );

        gl_module.module_internal().clear_context(&mut ctx);

        Ok(func_id)
    }

    /// Declare a function without defining it (useful for forward declarations)
    /// Returns the FuncId for later use in function calls
    pub fn declare_function<M: Module>(
        gl_module: &mut GlModule<M>,
        name: &str,
        linkage: Linkage,
        sig: Signature,
    ) -> Result<FuncId, GlslError> {
        gl_module.declare_function(name, linkage, sig)
    }
}
