//! Programmatic function building utilities
//!
//! **DEPRECATED**: These functions are kept for backward compatibility but
//! are deprecated. Use `test_helpers` module for tests instead.

#[cfg(test)]
pub use crate::backend::module::test_helpers::test_helpers::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::module::gl_module::GlModule;
    use crate::backend::target::Target;
    use cranelift_codegen::ir::{AbiParam, InstBuilder, Signature, types};
    use cranelift_codegen::isa::CallConv;
    use cranelift_module::Linkage;

    #[test]
    #[cfg(feature = "std")]
    fn test_build_simple_function() {
        let target = Target::host_jit().unwrap();
        let mut gl_module = GlModule::new_jit(target).unwrap();

        let mut sig = Signature::new(CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I32));

        let result =
            build_simple_function(&mut gl_module, "test", Linkage::Local, sig, |builder| {
                let _entry = builder.current_block().unwrap();
                let val = builder.ins().iconst(types::I32, 42);
                builder.ins().return_(&[val]);
                Ok(())
            });

        assert!(result.is_ok());
        assert!(gl_module.get_func("test").is_some());
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_build_function_with_params() {
        let target = Target::host_jit().unwrap();
        let mut gl_module = GlModule::new_jit(target).unwrap();

        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::I32));
        sig.params.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::I32));

        let result = build_simple_function(&mut gl_module, "add", Linkage::Local, sig, |builder| {
            let entry = builder.current_block().unwrap();
            let a = builder.block_params(entry)[0];
            let b = builder.block_params(entry)[1];
            let sum = builder.ins().iadd(a, b);
            builder.ins().return_(&[sum]);
            Ok(())
        });

        assert!(result.is_ok());
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_build_call_function() {
        let target = Target::host_jit().unwrap();
        let mut gl_module = GlModule::new_jit(target).unwrap();

        // First, build the callee
        let mut add_sig = Signature::new(CallConv::SystemV);
        add_sig.params.push(AbiParam::new(types::I32));
        add_sig.params.push(AbiParam::new(types::I32));
        add_sig.returns.push(AbiParam::new(types::I32));

        build_simple_function(&mut gl_module, "add", Linkage::Local, add_sig, |builder| {
            let entry = builder.current_block().unwrap();
            let a = builder.block_params(entry)[0];
            let b = builder.block_params(entry)[1];
            let sum = builder.ins().iadd(a, b);
            builder.ins().return_(&[sum]);
            Ok(())
        })
        .unwrap();

        // Then, build the caller
        let mut main_sig = Signature::new(CallConv::SystemV);
        main_sig.returns.push(AbiParam::new(types::I32));

        let result = build_call_function(
            &mut gl_module,
            "main",
            Linkage::Export,
            main_sig,
            "add",
            |builder| {
                let ten = builder.ins().iconst(types::I32, 10);
                let twenty = builder.ins().iconst(types::I32, 20);
                Ok(vec![ten, twenty])
            },
        );

        assert!(result.is_ok());
        assert!(gl_module.get_func("main").is_some());
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_declare_function() {
        let target = Target::host_jit().unwrap();
        let mut gl_module = GlModule::new_jit(target).unwrap();

        let mut sig = Signature::new(CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I32));

        let result = declare_function(&mut gl_module, "forward", Linkage::Local, sig);
        assert!(result.is_ok());
        assert!(gl_module.get_func("forward").is_some());
    }
}
