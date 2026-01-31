//! Type inference and validation for GLSL expressions
//! Implements GLSL spec type rules for Phase 3

pub mod constructors;
pub mod conversion;
pub mod inference;
pub mod matrix;
pub mod operators;
pub mod swizzle;

// Re-export public API
pub use constructors::{
    check_matrix_constructor, check_scalar_constructor_with_span, check_vector_constructor,
    check_vector_constructor_with_span, is_matrix_type_name, is_scalar_type_name,
    is_vector_type_name,
};
pub use conversion::{
    can_implicitly_convert, check_assignment, check_assignment_with_span, promote_numeric,
};
pub use inference::{infer_expr_type, infer_expr_type_in_context, infer_expr_type_with_registry};
pub use matrix::infer_matrix_binary_result_type;
pub use operators::{
    check_condition, infer_binary_result_type, infer_postdec_result_type,
    infer_postinc_result_type, infer_predec_result_type, infer_preinc_result_type,
    infer_unary_result_type,
};
pub use swizzle::parse_swizzle_length;
