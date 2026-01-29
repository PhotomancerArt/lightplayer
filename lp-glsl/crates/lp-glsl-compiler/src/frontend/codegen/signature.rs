//! Helper for building Cranelift function signatures from GLSL types.

use crate::semantic::functions::Parameter;
use crate::semantic::types::Type;
use cranelift_codegen::ir::{AbiParam, ArgumentPurpose, Signature, Type as IrType};
use cranelift_codegen::isa::CallConv;
use target_lexicon::Triple;

/// Builder for Cranelift function signatures from GLSL function signatures.
pub struct SignatureBuilder;

impl SignatureBuilder {
    /// Create a new empty signature with the default calling convention.
    /// This uses SystemV as a fallback. Prefer `new_with_triple()` for ISA-specific calling conventions.
    pub fn new() -> Signature {
        Signature::new(CallConv::SystemV)
    }

    /// Create a new empty signature with the calling convention appropriate for the given triple.
    pub fn new_with_triple(triple: &Triple) -> Signature {
        Signature::new(CallConv::triple_default(triple))
    }

    /// Build a complete signature from GLSL return type and parameters.
    /// `pointer_type` is required when the return type is a composite type (vector or matrix).
    ///
    /// Note: StructReturn parameter is added FIRST (before regular params) to match ABI requirements.
    pub fn build(return_type: &Type, parameters: &[Parameter], pointer_type: IrType) -> Signature {
        let mut sig = Self::new();
        // Add StructReturn FIRST if needed (before regular params, like cranelift-examples)
        Self::add_return_type(&mut sig, return_type, pointer_type);
        // Then add regular parameters
        Self::add_parameters(&mut sig, parameters, pointer_type);
        sig
    }

    /// Build a complete signature from GLSL return type and parameters with ISA-specific calling convention.
    /// `pointer_type` is required when the return type is a composite type (vector or matrix).
    /// `triple` is used to determine the correct calling convention for the target ISA.
    ///
    /// Note: StructReturn parameter is added FIRST (before regular params) to match ABI requirements.
    pub fn build_with_triple(
        return_type: &Type,
        parameters: &[Parameter],
        pointer_type: IrType,
        triple: &Triple,
    ) -> Signature {
        let mut sig = Self::new_with_triple(triple);
        // Add StructReturn FIRST if needed (before regular params, like cranelift-examples)
        Self::add_return_type(&mut sig, return_type, pointer_type);
        // Then add regular parameters
        Self::add_parameters(&mut sig, parameters, pointer_type);
        sig
    }

    /// Add parameters to a signature from GLSL parameters.
    pub fn add_parameters(sig: &mut Signature, parameters: &[Parameter], pointer_type: IrType) {
        for param in parameters {
            Self::add_type_as_params(sig, &param.ty, param.qualifier, pointer_type);
        }
    }

    /// Add return type to a signature.
    /// `pointer_type` is required when the return type is a composite type (vector or matrix).
    pub fn add_return_type(sig: &mut Signature, return_type: &Type, pointer_type: IrType) {
        if *return_type != Type::Void {
            Self::add_type_as_returns(sig, return_type, pointer_type);
        }
    }

    /// Add a GLSL type as parameters (expanding vectors/matrices into components).
    /// For out/inout parameters, passes a pointer instead of expanding to components.
    fn add_type_as_params(
        sig: &mut Signature,
        ty: &Type,
        qualifier: crate::semantic::functions::ParamQualifier,
        pointer_type: IrType,
    ) {
        use crate::semantic::functions::ParamQualifier;

        match qualifier {
            ParamQualifier::Out | ParamQualifier::InOut => {
                // Out/inout parameters: pass as single pointer (regardless of type)
                // For vectors/matrices, pointer points to first element
                sig.params.push(AbiParam::new(pointer_type));
            }
            ParamQualifier::In => {
                // In parameters: expand to components (existing behavior)
                if ty.is_vector() {
                    // Vector: pass each component as separate parameter
                    let base_ty = ty.vector_base_type().unwrap();
                    let cranelift_ty = base_ty
                        .to_cranelift_type()
                        .expect("vector base type should be convertible");
                    let count = ty.component_count().unwrap();
                    for _ in 0..count {
                        sig.params.push(AbiParam::new(cranelift_ty));
                    }
                } else if ty.is_matrix() {
                    // Matrix: pass each element as separate parameter (column-major)
                    let element_count = ty.matrix_element_count().unwrap();
                    let cranelift_ty = Type::Float
                        .to_cranelift_type()
                        .expect("Float type should be convertible");
                    for _ in 0..element_count {
                        sig.params.push(AbiParam::new(cranelift_ty));
                    }
                } else {
                    // Scalar: single parameter
                    let cranelift_ty = ty
                        .to_cranelift_type()
                        .expect("scalar type should be convertible");
                    sig.params.push(AbiParam::new(cranelift_ty));
                }
            }
        }
    }

    /// Add a GLSL type as return values.
    /// For composite types (vectors and matrices), uses StructReturn parameter instead.
    /// StructReturn parameter is added FIRST in the params list (before regular params).
    fn add_type_as_returns(sig: &mut Signature, ty: &Type, pointer_type: IrType) {
        if ty.is_vector() {
            // Vector: use StructReturn parameter instead of multiple return values
            // Add StructReturn parameter FIRST (like cranelift-examples)
            sig.params.insert(
                0,
                AbiParam::special(pointer_type, ArgumentPurpose::StructReturn),
            );
            // StructReturn functions return void
            sig.returns.clear();
        } else if ty.is_matrix() {
            // Matrix: use StructReturn parameter instead of multiple return values
            // Add StructReturn parameter FIRST (like cranelift-examples)
            sig.params.insert(
                0,
                AbiParam::special(pointer_type, ArgumentPurpose::StructReturn),
            );
            // StructReturn functions return void
            sig.returns.clear();
        } else {
            // Scalar: single return value (no StructReturn)
            let cranelift_ty = ty
                .to_cranelift_type()
                .expect("scalar return type should be convertible");
            sig.returns.push(AbiParam::new(cranelift_ty));
        }
    }

    /// Count how many Cranelift parameters a GLSL type will expand to.
    /// For out/inout parameters, returns 1 (pointer). For in parameters, expands to components.
    pub fn count_parameters(
        ty: &Type,
        qualifier: crate::semantic::functions::ParamQualifier,
    ) -> usize {
        use crate::semantic::functions::ParamQualifier;

        match qualifier {
            ParamQualifier::Out | ParamQualifier::InOut => {
                // Out/inout parameters: single pointer
                1
            }
            ParamQualifier::In => {
                // In parameters: expand to components
                if ty.is_vector() {
                    ty.component_count().unwrap()
                } else if ty.is_matrix() {
                    ty.matrix_element_count().unwrap()
                } else {
                    1
                }
            }
        }
    }

    /// Count how many Cranelift return values a GLSL type will expand to.
    /// Returns 0 for composite types (vectors/matrices) as they use StructReturn.
    pub fn count_returns(ty: &Type) -> usize {
        if ty == &Type::Void {
            0
        } else if ty.is_vector() {
            // Vectors use StructReturn, so no return values
            0
        } else if ty.is_matrix() {
            // Matrices use StructReturn, so no return values
            0
        } else {
            1
        }
    }
}
