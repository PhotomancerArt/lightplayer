//! Generator for op-multiply test files.

use crate::types::{Dimension, VecType};
use crate::util::generate_header;
use crate::vec::util::{format_type_name, format_vector_constructor};

/// Generate op-multiply test file content.
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Generate header with regeneration command
    let specifier = format!("vec/{type_name}/op-multiply");
    let mut content = generate_header(&specifier);

    // Add test run and target directives
    content.push_str("// test run\n");
    content.push_str("// target riscv32.q32\n");
    content.push_str("\n");

    // Add section comment
    content.push_str(&format!(
        "// ============================================================================\n"
    ));
    content.push_str(&format!(
        "// Multiply: {type_name} * {type_name} -> {type_name} (component-wise)\n"
    ));
    content.push_str(&format!(
        "// ============================================================================\n"
    ));
    content.push_str("\n");

    // Generate test cases
    content.push_str(&generate_test_positive_positive(vec_type, dimension));
    content.push_str("\n");
    let positive_negative_test = generate_test_positive_negative(vec_type, dimension);
    if !positive_negative_test.is_empty() {
        content.push_str(&positive_negative_test);
        content.push_str("\n");
    }
    let negative_negative_test = generate_test_negative_negative(vec_type, dimension);
    if !negative_negative_test.is_empty() {
        content.push_str(&negative_negative_test);
        content.push_str("\n");
    }
    content.push_str(&generate_test_by_zero(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_by_one(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_variables(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_expressions(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_in_assignment(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_large_numbers(vec_type, dimension));
    content.push_str("\n");
    let overflow_test = generate_test_overflow(vec_type, dimension);
    if !overflow_test.is_empty() {
        content.push_str(&overflow_test);
        content.push_str("\n");
    }
    let mixed_components_test = generate_test_mixed_components(vec_type, dimension);
    if !mixed_components_test.is_empty() {
        content.push_str(&mixed_components_test);
        content.push_str("\n");
    }
    content.push_str(&generate_test_fractions(vec_type, dimension));

    content
}

/// Returns the comparison operator to use for this vector type.
/// For floating point (vec), use ~= for approximate equality.
/// For integer (ivec, uvec), use == for exact equality.
fn comparison_operator(vec_type: VecType) -> &'static str {
    match vec_type {
        VecType::Vec => "~=",  // Floating point uses approximate equality
        VecType::IVec => "==", // Integer types use exact equality
        VecType::UVec => "==",
        VecType::BVec => "==",
    }
}

fn generate_test_positive_positive(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [6, 7, 2, 3...], b = [2, 3, 4, 5...]
    // Result: [12, 21, 8, 15...] (component-wise multiplication)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![6, 7],
        Dimension::D3 => vec![6, 7, 2],
        Dimension::D4 => vec![6, 7, 2, 3],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 3],
        Dimension::D3 => vec![2, 3, 4],
        Dimension::D4 => vec![2, 3, 4, 5],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![12, 21],
        Dimension::D3 => vec![12, 21, 8],
        Dimension::D4 => vec![12, 21, 8, 15],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_multiply_positive_positive() {{\n\
    // Multiplication with positive vectors (component-wise)\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_positive_positive() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_positive_negative(vec_type: VecType, dimension: Dimension) -> String {
    // Skip negative tests for unsigned types
    if matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [5, 4, 3, 2...], b = [-3, -2, -1, -4...]
    // Result: [-15, -8, -3, -8...] (component-wise multiplication)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 4],
        Dimension::D3 => vec![5, 4, 3],
        Dimension::D4 => vec![5, 4, 3, 2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-3, -2],
        Dimension::D3 => vec![-3, -2, -1],
        Dimension::D4 => vec![-3, -2, -1, -4],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![-15, -8],
        Dimension::D3 => vec![-15, -8, -3],
        Dimension::D4 => vec![-15, -8, -3, -8],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_multiply_positive_negative() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_positive_negative() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_negative_negative(vec_type: VecType, dimension: Dimension) -> String {
    // Skip negative tests for unsigned types
    if matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [-4, -5, -2, -3...], b = [-2, -3, -1, -2...]
    // Result: [8, 15, 2, 6...] (component-wise multiplication)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-4, -5],
        Dimension::D3 => vec![-4, -5, -2],
        Dimension::D4 => vec![-4, -5, -2, -3],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-2, -3],
        Dimension::D3 => vec![-2, -3, -1],
        Dimension::D4 => vec![-2, -3, -1, -2],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![8, 15],
        Dimension::D3 => vec![8, 15, 2],
        Dimension::D4 => vec![8, 15, 2, 6],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_multiply_negative_negative() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_negative_negative() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_by_zero(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [123, 456, 789, 321...], b = [0, 0, 0, 0...]
    // Result: [0, 0, 0, 0...] (multiply by zero)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![123, 456],
        Dimension::D3 => vec![123, 456, 789],
        Dimension::D4 => vec![123, 456, 789, 321],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![0, 0],
        Dimension::D3 => vec![0, 0, 0],
        Dimension::D4 => vec![0, 0, 0, 0],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![0, 0],
        Dimension::D3 => vec![0, 0, 0],
        Dimension::D4 => vec![0, 0, 0, 0],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_multiply_by_zero() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_by_zero() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_by_one(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [42, 17, 23, 8...], b = [1, 1, 1, 1...]
    // Result: [42, 17, 23, 8...] (multiply by one, unchanged)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![42, 17],
        Dimension::D3 => vec![42, 17, 23],
        Dimension::D4 => vec![42, 17, 23, 8],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 1],
        Dimension::D3 => vec![1, 1, 1],
        Dimension::D4 => vec![1, 1, 1, 1],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{type_name} test_{type_name}_multiply_by_one() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_by_one() {cmp_op} {a_constructor}\n"
    )
}

fn generate_test_variables(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [8, 9, 7, 6...], b = [7, 6, 5, 4...]
    // Result: [56, 54, 35, 24...] (component-wise multiplication)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![8, 9],
        Dimension::D3 => vec![8, 9, 7],
        Dimension::D4 => vec![8, 9, 7, 6],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 6],
        Dimension::D3 => vec![7, 6, 5],
        Dimension::D4 => vec![7, 6, 5, 4],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![56, 54],
        Dimension::D3 => vec![56, 54, 35],
        Dimension::D4 => vec![56, 54, 35, 24],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_multiply_variables() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_variables() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_expressions(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: [3, 4, 5, 2...] * [5, 2, 1, 6...]
    // Result: [15, 8, 5, 12...] (component-wise multiplication)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 4],
        Dimension::D3 => vec![3, 4, 5],
        Dimension::D4 => vec![3, 4, 5, 2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 2],
        Dimension::D3 => vec![5, 2, 1],
        Dimension::D4 => vec![5, 2, 1, 6],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![15, 8],
        Dimension::D3 => vec![15, 8, 5],
        Dimension::D4 => vec![15, 8, 5, 12],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_multiply_expressions() {{\n\
    return {a_constructor} * {b_constructor};\n\
}}\n\
\n\
// run: test_{type_name}_multiply_expressions() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_in_assignment(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: result = [6, 7, 8, 9...], then multiply by [2, 3, 1, 2...]
    // Result: [12, 21, 8, 18...] (component-wise multiplication)
    let initial_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![6, 7],
        Dimension::D3 => vec![6, 7, 8],
        Dimension::D4 => vec![6, 7, 8, 9],
    };
    let multiply_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 3],
        Dimension::D3 => vec![2, 3, 1],
        Dimension::D4 => vec![2, 3, 1, 2],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![12, 21],
        Dimension::D3 => vec![12, 21, 8],
        Dimension::D4 => vec![12, 21, 8, 18],
    };

    let initial_constructor = format_vector_constructor(vec_type, dimension, &initial_values);
    let multiply_constructor = format_vector_constructor(vec_type, dimension, &multiply_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_multiply_in_assignment() {{\n\
    {type_name} result = {initial_constructor};\n\
    result = result * {multiply_constructor};\n\
    return result;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_in_assignment() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_large_numbers(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [1000, 2000, 3000, 4000...], b = [3000, 1000, 2000, 500...]
    // For vec: may overflow, for ivec/uvec: exact arithmetic
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1000, 2000],
        Dimension::D3 => vec![1000, 2000, 3000],
        Dimension::D4 => vec![1000, 2000, 3000, 4000],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3000, 1000],
        Dimension::D3 => vec![3000, 1000, 2000],
        Dimension::D4 => vec![3000, 1000, 2000, 500],
    };

    let expected_values: Vec<i32> = match vec_type {
        VecType::Vec => {
            // For floating point, large values are clamped to 32768.0
            match dimension {
                Dimension::D2 => vec![32768, 32768],
                Dimension::D3 => vec![32768, 32768, 32768],
                Dimension::D4 => vec![32768, 32768, 32768, 32768],
            }
        }
        _ => {
            // For integer types, exact arithmetic
            match dimension {
                Dimension::D2 => vec![3000000, 2000000],
                Dimension::D3 => vec![3000000, 2000000, 6000000],
                Dimension::D4 => vec![3000000, 2000000, 6000000, 2000000],
            }
        }
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected_values);

    format!(
        "{type_name} test_{type_name}_multiply_large_numbers() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_large_numbers() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_mixed_components(vec_type: VecType, dimension: Dimension) -> String {
    // Skip mixed positive/negative tests for unsigned types
    if matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [2, -3, 4, -2...], b = [-4, 5, -2, 3...]
    // Result: [-8, -15, -8, -6...] (component-wise multiplication)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, -3],
        Dimension::D3 => vec![2, -3, 4],
        Dimension::D4 => vec![2, -3, 4, -2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-4, 5],
        Dimension::D3 => vec![-4, 5, -2],
        Dimension::D4 => vec![-4, 5, -2, 3],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![-8, -15],
        Dimension::D3 => vec![-8, -15, -8],
        Dimension::D4 => vec![-8, -15, -8, -6],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_multiply_mixed_components() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_mixed_components() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_overflow(vec_type: VecType, dimension: Dimension) -> String {
    // Only include overflow tests for unsigned integer types
    if !matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Test unsigned integer overflow behavior
    // Hardcode the constructors since the values are too large for i32
    let (a_constructor, b_constructor, expected_constructor) = match dimension {
        Dimension::D2 => (
            "uvec2(4294967295u, 4294967295u)".to_string(),
            "uvec2(2u, 2u)".to_string(),
            "uvec2(4294967294u, 4294967294u)".to_string(),
        ),
        Dimension::D3 => (
            "uvec3(4294967295u, 4294967295u, 4294967295u)".to_string(),
            "uvec3(2u, 2u, 2u)".to_string(),
            "uvec3(4294967294u, 4294967294u, 4294967294u)".to_string(),
        ),
        Dimension::D4 => (
            "uvec4(4294967295u, 4294967295u, 4294967295u, 4294967295u)".to_string(),
            "uvec4(2u, 2u, 2u, 2u)".to_string(),
            "uvec4(4294967294u, 4294967294u, 4294967294u, 4294967294u)".to_string(),
        ),
    };

    format!(
        "{type_name} test_{type_name}_multiply_overflow() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_overflow() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_fractions(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // For floating point types, use fractional values
    // For integer types, use regular integer values
    let (a_constructor, b_constructor, expected_constructor) = if matches!(vec_type, VecType::Vec) {
        match dimension {
            Dimension::D2 => (
                "vec2(1.5, 2.5)".to_string(),
                "vec2(2.0, 0.5)".to_string(),
                "vec2(3.0, 1.25)".to_string(),
            ),
            Dimension::D3 => (
                "vec3(1.5, 2.5, 3.5)".to_string(),
                "vec3(2.0, 0.5, 1.5)".to_string(),
                "vec3(3.0, 1.25, 5.25)".to_string(),
            ),
            Dimension::D4 => (
                "vec4(1.5, 2.5, 3.5, 0.5)".to_string(),
                "vec4(2.0, 0.5, 1.5, 4.0)".to_string(),
                "vec4(3.0, 1.25, 5.25, 2.0)".to_string(),
            ),
        }
    } else {
        // For integer types, use integer values
        match dimension {
            Dimension::D2 => (
                format_vector_constructor(vec_type, dimension, &[3, 4]),
                format_vector_constructor(vec_type, dimension, &[5, 2]),
                format_vector_constructor(vec_type, dimension, &[15, 8]),
            ),
            Dimension::D3 => (
                format_vector_constructor(vec_type, dimension, &[3, 4, 5]),
                format_vector_constructor(vec_type, dimension, &[5, 2, 1]),
                format_vector_constructor(vec_type, dimension, &[15, 8, 5]),
            ),
            Dimension::D4 => (
                format_vector_constructor(vec_type, dimension, &[3, 4, 5, 2]),
                format_vector_constructor(vec_type, dimension, &[5, 2, 1, 6]),
                format_vector_constructor(vec_type, dimension, &[15, 8, 5, 12]),
            ),
        }
    };

    format!(
        "{type_name} test_{type_name}_multiply_fractions() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{type_name}_multiply_fractions() {cmp_op} {expected_constructor}\n"
    )
}
