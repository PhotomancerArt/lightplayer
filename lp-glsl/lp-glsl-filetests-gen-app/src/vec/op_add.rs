//! Generator for op-add test files.

use crate::types::{Dimension, VecType};
use crate::util::generate_header;
use crate::vec::util::{format_type_name, format_vector_constructor};

/// Generate op-add test file content.
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Generate header with regeneration command
    let specifier = format!("vec/{type_name}/op-add");
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
        "// Add: {type_name} + {type_name} -> {type_name} (component-wise)\n"
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
    content.push_str(&generate_test_zero(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_variables(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_expressions(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_in_assignment(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_large_numbers(vec_type, dimension));
    content.push_str("\n");
    let max_values_test = generate_test_max_values(vec_type, dimension);
    if !max_values_test.is_empty() {
        content.push_str(&max_values_test);
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

    // Values: a = [5, 3, 2, 1...], b = [2, 4, 1, 3...]
    // Result: [7, 7, 3, 4...] (component-wise addition)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 4],
        Dimension::D3 => vec![2, 4, 1],
        Dimension::D4 => vec![2, 4, 1, 3],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 7],
        Dimension::D3 => vec![7, 7, 3],
        Dimension::D4 => vec![7, 7, 3, 4],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_add_positive_positive() {{\n\
    // Addition with positive vectors (component-wise)\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{type_name}_add_positive_positive() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_positive_negative(vec_type: VecType, dimension: Dimension) -> String {
    // Skip negative tests for unsigned types
    if matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [10, 8, 5, 3...], b = [-4, -2, -1, -3...]
    // Result: [6, 6, 4, 0...] (component-wise addition)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![10, 8],
        Dimension::D3 => vec![10, 8, 5],
        Dimension::D4 => vec![10, 8, 5, 3],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-4, -2],
        Dimension::D3 => vec![-4, -2, -1],
        Dimension::D4 => vec![-4, -2, -1, -3],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![6, 6],
        Dimension::D3 => vec![6, 6, 4],
        Dimension::D4 => vec![6, 6, 4, 0],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_add_positive_negative() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{type_name}_add_positive_negative() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_negative_negative(vec_type: VecType, dimension: Dimension) -> String {
    // Skip negative tests for unsigned types
    if matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [-3, -7, -2, -5...], b = [-2, -1, -3, -1...]
    // Result: [-5, -8, -5, -6...] (component-wise addition)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-3, -7],
        Dimension::D3 => vec![-3, -7, -2],
        Dimension::D4 => vec![-3, -7, -2, -5],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-2, -1],
        Dimension::D3 => vec![-2, -1, -3],
        Dimension::D4 => vec![-2, -1, -3, -1],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![-5, -8],
        Dimension::D3 => vec![-5, -8, -5],
        Dimension::D4 => vec![-5, -8, -5, -6],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_add_negative_negative() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{type_name}_add_negative_negative() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_zero(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [42, 17, 23, 8...], b = [0, 0, 0, 0...]
    // Result: [42, 17, 23, 8...] (adding zero doesn't change the value)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![42, 17],
        Dimension::D3 => vec![42, 17, 23],
        Dimension::D4 => vec![42, 17, 23, 8],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![0, 0],
        Dimension::D3 => vec![0, 0, 0],
        Dimension::D4 => vec![0, 0, 0, 0],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{type_name} test_{type_name}_add_zero() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{type_name}_add_zero() {cmp_op} {a_constructor}\n"
    )
}

fn generate_test_variables(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [15, 10, 5, 12...], b = [27, 5, 12, 3...]
    // Result: [42, 15, 17, 15...] (component-wise addition)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![15, 10],
        Dimension::D3 => vec![15, 10, 5],
        Dimension::D4 => vec![15, 10, 5, 12],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![27, 5],
        Dimension::D3 => vec![27, 5, 12],
        Dimension::D4 => vec![27, 5, 12, 3],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![42, 15],
        Dimension::D3 => vec![42, 15, 17],
        Dimension::D4 => vec![42, 15, 17, 15],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_add_variables() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{type_name}_add_variables() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_expressions(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: [8, 4, 6, 2...] + [6, 2, 3, 4...]
    // Result: [14, 6, 9, 6...] (component-wise addition)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![8, 4],
        Dimension::D3 => vec![8, 4, 6],
        Dimension::D4 => vec![8, 4, 6, 2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![6, 2],
        Dimension::D3 => vec![6, 2, 3],
        Dimension::D4 => vec![6, 2, 3, 4],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![14, 6],
        Dimension::D3 => vec![14, 6, 9],
        Dimension::D4 => vec![14, 6, 9, 6],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_add_expressions() {{\n\
    return {a_constructor} + {b_constructor};\n\
}}\n\
\n\
// run: test_{type_name}_add_expressions() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_in_assignment(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: result = [5, 3, 2, 1...], then add [10, 7, 8, 9...]
    // Result: [15, 10, 10, 10...] (component-wise addition)
    let initial_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let add_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![10, 7],
        Dimension::D3 => vec![10, 7, 8],
        Dimension::D4 => vec![10, 7, 8, 9],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![15, 10],
        Dimension::D3 => vec![15, 10, 10],
        Dimension::D4 => vec![15, 10, 10, 10],
    };

    let initial_constructor = format_vector_constructor(vec_type, dimension, &initial_values);
    let add_constructor = format_vector_constructor(vec_type, dimension, &add_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_add_in_assignment() {{\n\
    {type_name} result = {initial_constructor};\n\
    result = result + {add_constructor};\n\
    return result;\n\
}}\n\
\n\
// run: test_{type_name}_add_in_assignment() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_large_numbers(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Large numbers that may be clamped to fixed16x16 max (32767.99998)
    // Values: a = [100000, 50000, 25000, 10000...], b = [200000, 30000, 15000, 5000...]
    // For vec: result may be clamped, for ivec/uvec: exact arithmetic
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![100000, 50000],
        Dimension::D3 => vec![100000, 50000, 25000],
        Dimension::D4 => vec![100000, 50000, 25000, 10000],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![200000, 30000],
        Dimension::D3 => vec![200000, 30000, 15000],
        Dimension::D4 => vec![200000, 30000, 15000, 5000],
    };

    let expected_values: Vec<i32> = match vec_type {
        VecType::Vec => {
            // For floating point, values may be clamped
            match dimension {
                Dimension::D2 => vec![32768, 32768],
                Dimension::D3 => vec![32768, 32768, 32768], // 25000 + 15000 = 40000, but clamped to 32768
                Dimension::D4 => vec![32768, 32768, 32768, 15000], // 10000 + 5000 = 15000
            }
        }
        _ => {
            // For integer types, exact arithmetic
            match dimension {
                Dimension::D2 => vec![300000, 80000],
                Dimension::D3 => vec![300000, 80000, 40000],
                Dimension::D4 => vec![300000, 80000, 40000, 15000],
            }
        }
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected_values);

    format!(
        "{type_name} test_{type_name}_add_large_numbers() {{\n\
    // Large numbers are clamped to fixed16x16 max (32767.99998, rounds to 32768.0)\n\
    // Addition saturates to max for each component\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{type_name}_add_large_numbers() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_mixed_components(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // For unsigned types, use different positive values
    let (a_values, b_values, expected) = if matches!(vec_type, VecType::UVec) {
        // Values: a = [100, 50, 75, 25...], b = [200, 75, 150, 50...]
        // Result: [300, 125, 225, 75...] (component-wise addition)
        match dimension {
            Dimension::D2 => (vec![100, 50], vec![200, 75], vec![300, 125]),
            Dimension::D3 => (vec![100, 50, 75], vec![200, 75, 150], vec![300, 125, 225]),
            Dimension::D4 => (
                vec![100, 50, 75, 25],
                vec![200, 75, 150, 50],
                vec![300, 125, 225, 75],
            ),
        }
    } else {
        // Values: a = [1, -2, 3, -4...], b = [-3, 4, -1, 2...]
        // Result: [-2, 2, 2, -2...] (component-wise addition)
        match dimension {
            Dimension::D2 => (vec![1, -2], vec![-3, 4], vec![-2, 2]),
            Dimension::D3 => (vec![1, -2, 3], vec![-3, 4, -1], vec![-2, 2, 2]),
            Dimension::D4 => (vec![1, -2, 3, -4], vec![-3, 4, -1, 2], vec![-2, 2, 2, -2]),
        }
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_add_mixed_components() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{type_name}_add_mixed_components() {cmp_op} {expected_constructor}\n"
    )
}

fn generate_test_max_values(vec_type: VecType, dimension: Dimension) -> String {
    // Only include max values test for unsigned integer types
    if !matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    format!(
        "{} test_{}_add_max_values() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{}_add_max_values() {} {}\n",
        type_name,
        type_name,
        type_name,
        match dimension {
            Dimension::D2 => "uvec2(4294967295u, 4294967294u)".to_string(),
            Dimension::D3 => "uvec3(4294967295u, 4294967294u, 4294967293u)".to_string(),
            Dimension::D4 =>
                "uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u)".to_string(),
        },
        type_name,
        match dimension {
            Dimension::D2 => "uvec2(1u, 1u)".to_string(),
            Dimension::D3 => "uvec3(1u, 1u, 1u)".to_string(),
            Dimension::D4 => "uvec4(1u, 1u, 1u, 1u)".to_string(),
        },
        type_name,
        cmp_op,
        match dimension {
            Dimension::D2 => "uvec2(0u, 4294967295u)".to_string(),
            Dimension::D3 => "uvec3(0u, 4294967295u, 4294967294u)".to_string(),
            Dimension::D4 => "uvec4(0u, 4294967295u, 4294967294u, 4294967293u)".to_string(),
        }
    )
}

fn generate_test_fractions(vec_type: VecType, dimension: Dimension) -> String {
    // Skip fractions test for integer types (doesn't make sense for integers)
    if matches!(vec_type, VecType::UVec | VecType::IVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // For floating point, use actual fractional values
    // Values: a = [1.5, 2.25, 3.75, 0.5...], b = [0.5, 1.75, 0.25, 1.5...]
    // Result: [2.0, 4.0, 4.0, 2.0...] (component-wise addition)
    let (a_constructor, b_constructor, expected_constructor) = match dimension {
        Dimension::D2 => (
            "vec2(1.5, 2.25)".to_string(),
            "vec2(0.5, 1.75)".to_string(),
            "vec2(2.0, 4.0)".to_string(),
        ),
        Dimension::D3 => (
            "vec3(1.5, 2.25, 3.75)".to_string(),
            "vec3(0.5, 1.75, 0.25)".to_string(),
            "vec3(2.0, 4.0, 4.0)".to_string(),
        ),
        Dimension::D4 => (
            "vec4(1.5, 2.25, 3.75, 0.5)".to_string(),
            "vec4(0.5, 1.75, 0.25, 1.5)".to_string(),
            "vec4(2.0, 4.0, 4.0, 2.0)".to_string(),
        ),
    };

    format!(
        "{type_name} test_{type_name}_add_fractions() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{type_name}_add_fractions() {cmp_op} {expected_constructor}\n"
    )
}
