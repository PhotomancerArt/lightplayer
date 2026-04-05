//! Vector-specific utilities for test generation.

use crate::types::{Dimension, VecType};

/// Format a literal value based on vector type.
pub fn format_literal(value: i32, vec_type: VecType) -> String {
    match vec_type {
        VecType::Vec => format!("{value}.0"),
        VecType::IVec => format!("{value}"),
        VecType::UVec => format!("{value}u"),
        VecType::BVec => {
            if value != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
    }
}

/// Format the vector type name (e.g., "vec4", "ivec4", "uvec4", "bvec4").
pub fn format_type_name(vec_type: VecType, dimension: Dimension) -> String {
    let dim_str = match dimension {
        Dimension::D2 => "2",
        Dimension::D3 => "3",
        Dimension::D4 => "4",
    };

    let prefix = match vec_type {
        VecType::Vec => "vec",
        VecType::IVec => "ivec",
        VecType::UVec => "uvec",
        VecType::BVec => "bvec",
    };

    format!("{prefix}{dim_str}")
}

/// Format the return type name for comparison functions (always bvec).
pub fn format_bvec_type_name(dimension: Dimension) -> String {
    let dim_str = match dimension {
        Dimension::D2 => "2",
        Dimension::D3 => "3",
        Dimension::D4 => "4",
    };
    format!("bvec{dim_str}")
}

/// Generate a vector constructor call.
pub fn format_vector_constructor(
    vec_type: VecType,
    dimension: Dimension,
    values: &[i32],
) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let dim = dimension.as_usize();

    if values.len() != dim {
        panic!(
            "Expected {} values for {:?}, got {}",
            dim,
            dimension,
            values.len()
        );
    }

    let formatted_values: Vec<String> = values
        .iter()
        .map(|&v| format_literal(v, vec_type))
        .collect();

    format!("{}({})", type_name, formatted_values.join(", "))
}

/// Format a bvec expected value for run directive.
pub fn format_bvec_expected(values: Vec<bool>) -> String {
    let formatted: Vec<String> = values.iter().map(|&v| v.to_string()).collect();
    format!("bvec{}({})", values.len(), formatted.join(", "))
}

/// Format a bvec value for comment.
pub fn format_bvec_comment(values: Vec<bool>) -> String {
    let formatted: Vec<String> = values.iter().map(|&v| v.to_string()).collect();
    format!("({})", formatted.join(","))
}
