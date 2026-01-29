//! Generator for fn-min test files.

use crate::types::{Dimension, VecType};
use crate::util::generate_header;
use crate::vec::util::{format_type_name, format_vector_constructor};

/// Generate fn-min test file content.
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Generate header with regeneration command
    let specifier = format!("vec/{type_name}/fn-min");
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
        "// Min: min({type_name}, {type_name}) -> {type_name} (component-wise minimum)\n"
    ));
    content.push_str(&format!(
        "// ============================================================================\n"
    ));
    content.push_str("\n");

    // Generate test cases
    content.push_str(&generate_test_first_smaller(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_second_smaller(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_mixed(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_equal(vec_type, dimension));
    content.push_str("\n");

    // Negative test only for signed types
    let negative_test = generate_test_negative(vec_type, dimension);
    if !negative_test.is_empty() {
        content.push_str(&negative_test);
        content.push_str("\n");
    }

    content.push_str(&generate_test_zero(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_variables(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_expressions(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_in_expression(vec_type, dimension));

    content
}

fn generate_test_first_smaller(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [3, 8, 5, 1...], b = [7, 4, 9, 6...]
    // Result: [3, 4, 5, 1...] (min of each component)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 8],
        Dimension::D3 => vec![3, 8, 5],
        Dimension::D4 => vec![3, 8, 5, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 4],
        Dimension::D3 => vec![7, 4, 9],
        Dimension::D4 => vec![7, 4, 9, 6],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 4],
        Dimension::D3 => vec![3, 4, 5],
        Dimension::D4 => vec![3, 4, 5, 1],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_min_first_smaller() {{\n\
    // Function min() returns {type_name} (component-wise minimum)\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return min(a, b);\n\
}}\n\
\n\
// run: test_{type_name}_min_first_smaller() == {expected_constructor}\n"
    )
}

fn generate_test_second_smaller(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [7, 8, 9, 6...], b = [3, 4, 5, 1...]
    // Result: [3, 4, 5, 1...] (min of each component)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 8],
        Dimension::D3 => vec![7, 8, 9],
        Dimension::D4 => vec![7, 8, 9, 6],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 4],
        Dimension::D3 => vec![3, 4, 5],
        Dimension::D4 => vec![3, 4, 5, 1],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 4],
        Dimension::D3 => vec![3, 4, 5],
        Dimension::D4 => vec![3, 4, 5, 1],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_min_second_smaller() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return min(a, b);\n\
}}\n\
\n\
// run: test_{type_name}_min_second_smaller() == {expected_constructor}\n"
    )
}

fn generate_test_mixed(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [3, 8, 2, 7...], b = [7, 4, 9, 3...]
    // Result: [3, 4, 2, 3...] (min of each component)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 8],
        Dimension::D3 => vec![3, 8, 2],
        Dimension::D4 => vec![3, 8, 2, 7],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 4],
        Dimension::D3 => vec![7, 4, 9],
        Dimension::D4 => vec![7, 4, 9, 3],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 4],
        Dimension::D3 => vec![3, 4, 2],
        Dimension::D4 => vec![3, 4, 2, 3],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_min_mixed() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return min(a, b);\n\
}}\n\
\n\
// run: test_{type_name}_min_mixed() == {expected_constructor}\n"
    )
}

fn generate_test_equal(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [5, 5, 5, 5...], b = [5, 5, 5, 5...]
    // Result: [5, 5, 5, 5...] (min of equal values)
    let values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 5],
        Dimension::D3 => vec![5, 5, 5],
        Dimension::D4 => vec![5, 5, 5, 5],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &values);

    format!(
        "{type_name} test_{type_name}_min_equal() {{\n\
    {type_name} a = {constructor};\n\
    {type_name} b = {constructor};\n\
    return min(a, b);\n\
}}\n\
\n\
// run: test_{type_name}_min_equal() == {constructor}\n"
    )
}

fn generate_test_negative(vec_type: VecType, dimension: Dimension) -> String {
    // Skip negative tests for unsigned types
    if matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [-3, -8, -2, -1...], b = [-7, -4, -9, -6...]
    // Result: [-7, -8, -9, -6...] (min of negative values)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-3, -8],
        Dimension::D3 => vec![-3, -8, -2],
        Dimension::D4 => vec![-3, -8, -2, -1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-7, -4],
        Dimension::D3 => vec![-7, -4, -9],
        Dimension::D4 => vec![-7, -4, -9, -6],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![-7, -8],
        Dimension::D3 => vec![-7, -8, -9],
        Dimension::D4 => vec![-7, -8, -9, -6],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_min_negative() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return min(a, b);\n\
}}\n\
\n\
// run: test_{type_name}_min_negative() == {expected_constructor}\n"
    )
}

fn generate_test_zero(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // For unsigned types, use all non-negative values
    let (a_values, b_values, expected) = if matches!(vec_type, VecType::UVec) {
        // Values: a = [0, 5, 1, 2...], b = [2, 3, 0, 1...]
        // Result: [0, 3, 0, 1...] (min including zeros)
        match dimension {
            Dimension::D2 => (vec![0, 5], vec![2, 3], vec![0, 3]),
            Dimension::D3 => (vec![0, 5, 1], vec![2, 3, 0], vec![0, 3, 0]),
            Dimension::D4 => (vec![0, 5, 1, 2], vec![2, 3, 0, 1], vec![0, 3, 0, 1]),
        }
    } else {
        // Values: a = [0, 5, -3, 2...], b = [2, -1, 0, -4...]
        // Result: [0, -1, -3, -4...] (min including zeros and negatives)
        match dimension {
            Dimension::D2 => (vec![0, 5], vec![2, -1], vec![0, -1]),
            Dimension::D3 => (vec![0, 5, -3], vec![2, -1, 0], vec![0, -1, -3]),
            Dimension::D4 => (vec![0, 5, -3, 2], vec![2, -1, 0, -4], vec![0, -1, -3, -4]),
        }
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_min_zero() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return min(a, b);\n\
}}\n\
\n\
// run: test_{type_name}_min_zero() == {expected_constructor}\n"
    )
}

fn generate_test_variables(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [10, 15, 8, 12...], b = [12, 10, 12, 8...]
    // Result: [10, 10, 8, 8...] (min of variables)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![10, 15],
        Dimension::D3 => vec![10, 15, 8],
        Dimension::D4 => vec![10, 15, 8, 12],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![12, 10],
        Dimension::D3 => vec![12, 10, 12],
        Dimension::D4 => vec![12, 10, 12, 8],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![10, 10],
        Dimension::D3 => vec![10, 10, 8],
        Dimension::D4 => vec![10, 10, 8, 8],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_min_variables() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return min(a, b);\n\
}}\n\
\n\
// run: test_{type_name}_min_variables() == {expected_constructor}\n"
    )
}

fn generate_test_expressions(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: [6, 2, 8, 4...] vs [4, 7, 3, 9...]
    // Result: [4, 2, 3, 4...] (min of inline expressions)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![6, 2],
        Dimension::D3 => vec![6, 2, 8],
        Dimension::D4 => vec![6, 2, 8, 4],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![4, 7],
        Dimension::D3 => vec![4, 7, 3],
        Dimension::D4 => vec![4, 7, 3, 9],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![4, 2],
        Dimension::D3 => vec![4, 2, 3],
        Dimension::D4 => vec![4, 2, 3, 4],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_min_expressions() {{\n\
    return min({a_constructor}, {b_constructor});\n\
}}\n\
\n\
// run: test_{type_name}_min_expressions() == {expected_constructor}\n"
    )
}

fn generate_test_in_expression(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [3, 8, 5, 2...], b = [7, 4, 9, 7...], c = [1, 6, 2, 3...]
    // Result: min(a, min(b, c)) = [1, 4, 2, 2...] (nested min calls)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 8],
        Dimension::D3 => vec![3, 8, 5],
        Dimension::D4 => vec![3, 8, 5, 2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 4],
        Dimension::D3 => vec![7, 4, 9],
        Dimension::D4 => vec![7, 4, 9, 7],
    };
    let c_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 6],
        Dimension::D3 => vec![1, 6, 2],
        Dimension::D4 => vec![1, 6, 2, 3],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 4],
        Dimension::D3 => vec![1, 4, 2],
        Dimension::D4 => vec![1, 4, 2, 2],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let c_constructor = format_vector_constructor(vec_type, dimension, &c_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{type_name} test_{type_name}_min_in_expression() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    {type_name} c = {c_constructor};\n\
    return min(a, min(b, c));\n\
}}\n\
\n\
// run: test_{type_name}_min_in_expression() == {expected_constructor}\n"
    )
}
