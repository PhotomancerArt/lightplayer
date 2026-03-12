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
    /// `scalar_float_type` is F32 for float mode, I32 for Q32 fixed-point mode.
    pub fn build(
        return_type: &Type,
        parameters: &[Parameter],
        pointer_type: IrType,
        scalar_float_type: IrType,
    ) -> Signature {
        let mut sig = Self::new();
        Self::add_return_type(&mut sig, return_type, pointer_type, scalar_float_type);
        Self::add_parameters(&mut sig, parameters, pointer_type, scalar_float_type);
        sig
    }

    /// Build a complete signature from GLSL return type and parameters with ISA-specific calling convention.
    /// `pointer_type` is required when the return type is a composite type (vector or matrix).
    /// `scalar_float_type` is F32 for float mode, I32 for Q32 fixed-point mode.
    pub fn build_with_triple(
        return_type: &Type,
        parameters: &[Parameter],
        pointer_type: IrType,
        triple: &Triple,
        scalar_float_type: IrType,
    ) -> Signature {
        let mut sig = Self::new_with_triple(triple);
        Self::add_return_type(&mut sig, return_type, pointer_type, scalar_float_type);
        Self::add_parameters(&mut sig, parameters, pointer_type, scalar_float_type);
        sig
    }

    /// Add parameters to a signature from GLSL parameters.
    pub fn add_parameters(
        sig: &mut Signature,
        parameters: &[Parameter],
        pointer_type: IrType,
        scalar_float_type: IrType,
    ) {
        for param in parameters {
            Self::add_type_as_params(
                sig,
                &param.ty,
                param.qualifier,
                pointer_type,
                scalar_float_type,
            );
        }
    }

    /// Add return type to a signature.
    pub fn add_return_type(
        sig: &mut Signature,
        return_type: &Type,
        pointer_type: IrType,
        scalar_float_type: IrType,
    ) {
        if *return_type != Type::Void {
            Self::add_type_as_returns(sig, return_type, pointer_type, scalar_float_type);
        }
    }

    /// Add a GLSL type as parameters (expanding vectors/matrices into components).
    /// For out/inout parameters, passes a pointer instead of expanding to components.
    fn add_type_as_params(
        sig: &mut Signature,
        ty: &Type,
        qualifier: crate::semantic::functions::ParamQualifier,
        pointer_type: IrType,
        scalar_float_type: IrType,
    ) {
        use crate::semantic::functions::ParamQualifier;

        match qualifier {
            ParamQualifier::Out | ParamQualifier::InOut => {
                sig.params.push(AbiParam::new(pointer_type));
            }
            ParamQualifier::In => {
                if ty.is_array() {
                    sig.params.push(AbiParam::new(pointer_type));
                } else if ty.is_vector() {
                    let base_ty = ty.vector_base_type().unwrap();
                    let cranelift_ty = if base_ty == Type::Float {
                        scalar_float_type
                    } else {
                        base_ty
                            .to_cranelift_type()
                            .expect("vector base type should be convertible")
                    };
                    let count = ty.component_count().unwrap();
                    for _ in 0..count {
                        sig.params.push(AbiParam::new(cranelift_ty));
                    }
                } else if ty.is_matrix() {
                    for _ in 0..ty.matrix_element_count().unwrap() {
                        sig.params.push(AbiParam::new(scalar_float_type));
                    }
                } else {
                    let cranelift_ty = if *ty == Type::Float {
                        scalar_float_type
                    } else {
                        ty.to_cranelift_type()
                            .expect("scalar type should be convertible")
                    };
                    sig.params.push(AbiParam::new(cranelift_ty));
                }
            }
        }
    }

    /// Add a GLSL type as return values.
    fn add_type_as_returns(
        sig: &mut Signature,
        ty: &Type,
        pointer_type: IrType,
        scalar_float_type: IrType,
    ) {
        if ty.is_vector() {
            sig.params.insert(
                0,
                AbiParam::special(pointer_type, ArgumentPurpose::StructReturn),
            );
            sig.returns.clear();
        } else if ty.is_matrix() {
            sig.params.insert(
                0,
                AbiParam::special(pointer_type, ArgumentPurpose::StructReturn),
            );
            sig.returns.clear();
        } else {
            let cranelift_ty = if *ty == Type::Float {
                scalar_float_type
            } else {
                ty.to_cranelift_type()
                    .expect("scalar return type should be convertible")
            };
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
                // In parameters: expand to components, or 1 for array (pointer)
                if ty.is_array() {
                    1
                } else if ty.is_vector() {
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
