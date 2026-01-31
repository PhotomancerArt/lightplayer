//! Transform pipeline trait and context
//!
//! This module defines the Transform trait that all transformations must implement.
//! Transformations consume a GlModule and produce a new GlModule with transformed functions.

use crate::backend::module::gl_module::GlModule;
use crate::error::GlslError;
use alloc::string::String;
use cranelift_codegen::ir::{Function, Signature};
use cranelift_module::{FuncId, Module};
use hashbrown::HashMap;

/// Context for transformations
///
/// Provides access to the new module and function reference mappings
/// needed during function transformation.
pub struct TransformContext<'a, M: Module> {
    /// The new module being built
    pub module: &'a mut GlModule<M>,
    /// Mapping from function names to their new FuncIds (for creating FuncRefs per function)
    pub func_id_map: HashMap<String, FuncId>,
    /// Mapping from old FuncIds to function names (for mapping UserExternalName references)
    pub old_func_id_map: HashMap<FuncId, String>,
}

/// Transform trait for module-level transformations
///
/// Transformations implement this trait to transform functions from one
/// representation to another (e.g., float to fixed-point).
pub trait Transform {
    /// Transform a function signature
    ///
    /// Converts the signature from the old representation to the new representation.
    fn transform_signature(&self, sig: &Signature) -> Signature;

    /// Transform a function body
    ///
    /// Converts the function body from the old representation to the new representation.
    /// The function may reference other functions via FuncRefs, which should be looked up
    /// in the `func_ref_map` provided in the context.
    fn transform_function<M: Module>(
        &self,
        func: &Function,
        ctx: &mut TransformContext<'_, M>,
    ) -> Result<Function, GlslError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::module::gl_module::GlModule;
    use crate::backend::target::Target;
    use cranelift_codegen::ir::{AbiParam, Signature, types};
    use cranelift_codegen::isa::CallConv;

    /// Identity transform for testing
    struct IdentityTransform;

    impl Transform for IdentityTransform {
        fn transform_signature(&self, sig: &Signature) -> Signature {
            sig.clone()
        }

        fn transform_function<M: Module>(
            &self,
            func: &Function,
            _ctx: &mut TransformContext<'_, M>,
        ) -> Result<Function, GlslError> {
            Ok(func.clone())
        }
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_identity_transform() {
        let target = Target::host_jit().unwrap();
        let mut gl_module = GlModule::new_jit(target).unwrap();

        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::I32));

        // Add a function
        let mut func = cranelift_codegen::ir::Function::new();
        func.signature = sig.clone();
        gl_module
            .add_function("test", cranelift_module::Linkage::Local, sig.clone(), func)
            .unwrap();

        // Apply identity transform
        let transform = IdentityTransform;
        let transformed = gl_module.apply_transform(transform).unwrap();

        // Verify function still exists
        assert!(transformed.get_func("test").is_some());
        let transformed_func = transformed.get_func("test").unwrap();
        assert_eq!(transformed_func.clif_sig, sig);
    }
}
