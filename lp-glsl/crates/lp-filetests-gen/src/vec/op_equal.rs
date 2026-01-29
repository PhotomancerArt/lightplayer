//! Generator for op-equal test files.

use crate::types::{Dimension, VecType};
use crate::util::generate_header;
use crate::vec::util::{
    format_bvec_expected, format_bvec_type_name, format_type_name, format_vector_constructor,
};

/// Generate op-equal test file content.
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Generate header with regeneration command
    let specifier = format!("vec/{type_name}/op-equal");
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
        "// Equal: == operator -> bool (aggregate), equal({}, {}) -> {} (component-wise)\n",
        type_name,
        type_name,
        format_bvec_type_name(dimension)
    ));
    content.push_str(&format!(
        "// ============================================================================\n"
    ));
    content.push_str("\n");

    // Generate operator tests (return bool)
    content.push_str(&generate_test_operator_true(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_operator_false(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_operator_partial_match(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_operator_all_zero(vec_type, dimension));
    content.push_str("\n");
    let negative_test = generate_test_operator_negative(vec_type, dimension);
    if !negative_test.is_empty() {
        content.push_str(&negative_test);
        content.push_str("\n");
    }
    content.push_str(&generate_test_operator_after_assignment(
        vec_type, dimension,
    ));
    content.push_str("\n");

    // Generate function tests (return bvec)
    content.push_str(&generate_test_function(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_function_all_true(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_function_all_false(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_function_mixed(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_function_floats(vec_type, dimension));

    content
}

fn generate_test_operator_true(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [5, 3, 2, 1...], b = [5, 3, 2, 1...]
    // Result: true (all components match)
    let values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &values);

    format!(
        "bool test_{type_name}_equal_operator_true() {{\n\
    {type_name} a = {constructor};\n\
    {type_name} b = {constructor};\n\
    // Operator == returns bool (aggregate comparison - all components must match)\n\
    return a == b;\n\
}}\n\
\n\
// run: test_{type_name}_equal_operator_true() == true\n"
    )
}

fn generate_test_operator_false(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [5, 3, 2, 1...], b = [2, 4, 1, 3...]
    // Result: false (no components match)
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

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "bool test_{type_name}_equal_operator_false() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a == b;\n\
}}\n\
\n\
// run: test_{type_name}_equal_operator_false() == false\n"
    )
}

fn generate_test_operator_partial_match(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [5, 3, 2, 1...], b = [5, 3, 2, 4...]
    // Result: false (not all components match)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 4],
        Dimension::D3 => vec![5, 3, 4],
        Dimension::D4 => vec![5, 3, 2, 4],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "bool test_{type_name}_equal_operator_partial_match() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    return a == b;\n\
}}\n\
\n\
// run: test_{type_name}_equal_operator_partial_match() == false\n"
    )
}

fn generate_test_operator_all_zero(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [0, 0, 0, 0...], b = [0, 0, 0, 0...]
    // Result: true (all components match, all zero)
    let values: Vec<i32> = match dimension {
        Dimension::D2 => vec![0, 0],
        Dimension::D3 => vec![0, 0, 0],
        Dimension::D4 => vec![0, 0, 0, 0],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &values);

    format!(
        "bool test_{type_name}_equal_operator_all_zero() {{\n\
    {type_name} a = {constructor};\n\
    {type_name} b = {constructor};\n\
    return a == b;\n\
}}\n\
\n\
// run: test_{type_name}_equal_operator_all_zero() == true\n"
    )
}

fn generate_test_operator_negative(vec_type: VecType, dimension: Dimension) -> String {
    // Skip negative tests for unsigned types
    if matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [-5, -3, -2, -1...], b = [-5, -3, -2, -1...]
    // Result: true (all components match, all negative)
    let values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-5, -3],
        Dimension::D3 => vec![-5, -3, -2],
        Dimension::D4 => vec![-5, -3, -2, -1],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &values);

    format!(
        "bool test_{type_name}_equal_operator_negative() {{\n\
    {type_name} a = {constructor};\n\
    {type_name} b = {constructor};\n\
    return a == b;\n\
}}\n\
\n\
// run: test_{type_name}_equal_operator_negative() == true\n"
    )
}

fn generate_test_operator_after_assignment(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [5, 3, 2, 1...], b = [2, 4, 1, 3...], then b = a
    // Result: true (after assignment, they are equal)
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

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "bool test_{type_name}_equal_operator_after_assignment() {{\n\
    {type_name} a = {a_constructor};\n\
    {type_name} b = {b_constructor};\n\
    b = a;\n\
    return a == b;\n\
}}\n\
\n\
// run: test_{type_name}_equal_operator_after_assignment() == true\n"
    )
}

fn generate_test_function(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 3, 2, 1...], b = [5, 4, 2, 1...]
    // Result: bvec4(true, false, true, true) (component-wise)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 4],
        Dimension::D3 => vec![5, 4, 2],
        Dimension::D4 => vec![5, 4, 2, 1],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    // Function equal() returns {} (component-wise comparison)\n\
    return equal(a, b);\n\
}}\n\
\n\
// run: test_{}_equal_function() == {}\n",
        bvec_type_name,
        type_name,
        type_name,
        a_constructor,
        type_name,
        b_constructor,
        bvec_type_name,
        type_name,
        format_bvec_expected(expected)
    )
}

fn generate_test_function_all_true(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [10, 20, 30, 40...], b = [10, 20, 30, 40...]
    // Result: bvec4(true, true, true, true) (all components equal)
    let values: Vec<i32> = match dimension {
        Dimension::D2 => vec![10, 20],
        Dimension::D3 => vec![10, 20, 30],
        Dimension::D4 => vec![10, 20, 30, 40],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, true],
        Dimension::D3 => vec![true, true, true],
        Dimension::D4 => vec![true, true, true, true],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &values);

    format!(
        "{} test_{}_equal_function_all_true() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return equal(a, b);\n\
}}\n\
\n\
// run: test_{}_equal_function_all_true() == {}\n",
        bvec_type_name,
        type_name,
        type_name,
        constructor,
        type_name,
        constructor,
        type_name,
        format_bvec_expected(expected)
    )
}

fn generate_test_function_all_false(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 3, 2, 1...], b = [2, 4, 1, 3...]
    // Result: bvec4(false, false, false, false) (no components equal)
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
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![false, false],
        Dimension::D3 => vec![false, false, false],
        Dimension::D4 => vec![false, false, false, false],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function_all_false() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return equal(a, b);\n\
}}\n\
\n\
// run: test_{}_equal_function_all_false() == {}\n",
        bvec_type_name,
        type_name,
        type_name,
        a_constructor,
        type_name,
        b_constructor,
        type_name,
        format_bvec_expected(expected)
    )
}

fn generate_test_function_mixed(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 3, 2, 1...], b = [2, 3, 4, 1...]
    // Result: bvec4(false, true, false, true) (mixed equal/unequal)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 3],
        Dimension::D3 => vec![2, 3, 4],
        Dimension::D4 => vec![2, 3, 4, 1],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![false, true],
        Dimension::D3 => vec![false, true, false],
        Dimension::D4 => vec![false, true, false, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function_mixed() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return equal(a, b);\n\
}}\n\
\n\
// run: test_{}_equal_function_mixed() == {}\n",
        bvec_type_name,
        type_name,
        type_name,
        a_constructor,
        type_name,
        b_constructor,
        type_name,
        format_bvec_expected(expected)
    )
}

fn generate_test_function_floats(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [1.5, 2.25, 3.75, 0.5...], b = [1.5, 2.25, 3.75, 0.5...]
    // Result: bvec4(true, true, true, true) (all components equal)
    let values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 2], // Will be formatted as floats
        Dimension::D3 => vec![1, 2, 3],
        Dimension::D4 => vec![1, 2, 3, 0],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, true],
        Dimension::D3 => vec![true, true, true],
        Dimension::D4 => vec![true, true, true, true],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &values);

    format!(
        "{} test_{}_equal_function_floats() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return equal(a, b);\n\
}}\n\
\n\
// run: test_{}_equal_function_floats() == {}\n",
        bvec_type_name,
        type_name,
        type_name,
        constructor,
        type_name,
        constructor,
        type_name,
        format_bvec_expected(expected)
    )
}
