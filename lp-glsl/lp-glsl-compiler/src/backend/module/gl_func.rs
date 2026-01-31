//! Function metadata structure

use alloc::string::String;
use cranelift_codegen::ir::{Function, Signature};
use cranelift_module::FuncId;

/// Function metadata and IR
pub struct GlFunc {
    pub name: String,
    pub clif_sig: Signature,
    pub func_id: FuncId,
    pub function: Function, // Function IR stored here
                            // Note: GLSL signature not needed for Phase 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::{AbiParam, types};
    use cranelift_codegen::isa::CallConv;

    #[test]
    fn test_gl_func_creation() {
        let mut sig = Signature::new(CallConv::SystemV);
        sig.params.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::I32));

        // Note: FuncId creation requires a Module, so this is a minimal test
        // This test just verifies the structure can be created with valid data
        let mut func = Function::new();
        func.signature = sig.clone();

        let _gl_func = GlFunc {
            name: String::from("test"),
            clif_sig: sig,
            func_id: FuncId::from_u32(0), // Dummy ID for test
            function: func,
        };

        assert_eq!(_gl_func.name, "test");
        assert_eq!(_gl_func.clif_sig.params.len(), 1);
    }
}
