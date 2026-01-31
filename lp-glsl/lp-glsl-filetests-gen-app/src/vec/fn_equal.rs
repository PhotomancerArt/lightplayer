//! Generator for fn-equal test files.

use crate::types::{Dimension, VecType};
use crate::util::generate_header;
use crate::vec::util::{
    format_bvec_comment, format_bvec_expected, format_bvec_type_name, format_type_name,
    format_vector_constructor,
};

/// Generate fn-equal test file content.
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Generate header with regeneration command
    let specifier = format!("vec/{type_name}/fn-equal");
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
        "// Equal: equal({type_name}, {type_name}) -> {bvec_type_name} (component-wise)\n"
    ));
    content.push_str(&format!(
        "// ============================================================================\n"
    ));
    content.push_str("\n");

    // Generate test cases
    content.push_str(&generate_test_mixed(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_all_true(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_all_false(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_zero(vec_type, dimension));
    content.push_str("\n");

    // Max values test only for unsigned types
    let max_values_test = generate_test_max_values(vec_type, dimension);
    if !max_values_test.is_empty() {
        content.push_str(&max_values_test);
        content.push_str("\n");
    }

    // Negative test only for signed types
    let negative_test = generate_test_negative(vec_type, dimension);
    if !negative_test.is_empty() {
        content.push_str(&negative_test);
        content.push_str("\n");
    }

    content.push_str(&generate_test_variables(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_expressions(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_in_expression(vec_type, dimension));

    content
}

fn generate_test_mixed(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 3, 7, 2...], b = [5, 4, 7, 2...]
    // Result: [true, false, true, true...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 7],
        Dimension::D4 => vec![5, 3, 7, 2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 4],
        Dimension::D3 => vec![5, 4, 7],
        Dimension::D4 => vec![5, 4, 7, 2],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function_mixed() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         // Function equal() returns {} (component-wise equality)\n\
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
        bvec_type_name,
        type_name,
        format_bvec_expected(expected)
    )
}

fn generate_test_all_true(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [10, 20, 30, 40...], b = [10, 20, 30, 40...]
    // Result: [true, true, true, true...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![10, 20],
        Dimension::D3 => vec![10, 20, 30],
        Dimension::D4 => vec![10, 20, 30, 40],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, true],
        Dimension::D3 => vec![true, true, true],
        Dimension::D4 => vec![true, true, true, true],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &a_values);

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

fn generate_test_all_false(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 3, 7, 2...], b = [2, 4, 1, 3...]
    // Result: [false, false, false, false...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 7],
        Dimension::D4 => vec![5, 3, 7, 2],
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

fn generate_test_zero(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [0, 5, 0, 2...], b = [0, 3, 1, 2...]
    // Result: [true, false, false, true...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![0, 5],
        Dimension::D3 => vec![0, 5, 0],
        Dimension::D4 => vec![0, 5, 0, 2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![0, 3],
        Dimension::D3 => vec![0, 3, 1],
        Dimension::D4 => vec![0, 3, 1, 2],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, false],
        Dimension::D4 => vec![true, false, false, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function_zero() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return equal(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_equal_function_zero() == {}\n",
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

fn generate_test_negative(vec_type: VecType, dimension: Dimension) -> String {
    // Skip negative tests for unsigned types
    if matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [-5, -3, -7, -2...], b = [-5, -4, -7, -1...]
    // Result: [true, false, true, false...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-5, -3],
        Dimension::D3 => vec![-5, -3, -7],
        Dimension::D4 => vec![-5, -3, -7, -2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-5, -4],
        Dimension::D3 => vec![-5, -4, -7],
        Dimension::D4 => vec![-5, -4, -7, -1],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, false],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function_negative() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return equal(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_equal_function_negative() == {}\n",
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

fn generate_test_variables(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [8, 12, 6, 9...], b = [8, 10, 7, 9...]
    // Result: [true, false, false, true...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![8, 12],
        Dimension::D3 => vec![8, 12, 6],
        Dimension::D4 => vec![8, 12, 6, 9],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![8, 10],
        Dimension::D3 => vec![8, 10, 7],
        Dimension::D4 => vec![8, 10, 7, 9],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, false],
        Dimension::D4 => vec![true, false, false, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function_variables() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return equal(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_equal_function_variables() == {}\n",
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

fn generate_test_expressions(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: [2, 5, 3, 8...] vs [2, 4, 8, 8...]
    // Result: [true, false, false, true...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 5],
        Dimension::D3 => vec![2, 5, 3],
        Dimension::D4 => vec![2, 5, 3, 8],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 4],
        Dimension::D3 => vec![2, 4, 8],
        Dimension::D4 => vec![2, 4, 8, 8],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, false],
        Dimension::D4 => vec![true, false, false, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function_expressions() {{\n\
         return equal({}, {});\n\
         }}\n\
         \n\
         // run: test_{}_equal_function_expressions() == {}\n",
        bvec_type_name,
        type_name,
        a_constructor,
        b_constructor,
        type_name,
        format_bvec_expected(expected)
    )
}

fn generate_test_max_values(vec_type: VecType, dimension: Dimension) -> String {
    // Only include max values test for unsigned integer types
    if !matches!(vec_type, VecType::UVec) {
        return String::new();
    }

    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    format!(
        "{} test_{}_equal_function_max_values() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return equal(a, b);\n\
}}\n\
\n\
// run: test_{}_equal_function_max_values() == {}\n",
        bvec_type_name,
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
            Dimension::D2 => "uvec2(4294967295u, 4294967294u)".to_string(),
            Dimension::D3 => "uvec3(4294967295u, 4294967294u, 4294967293u)".to_string(),
            Dimension::D4 =>
                "uvec4(4294967295u, 4294967294u, 4294967293u, 4294967292u)".to_string(),
        },
        type_name,
        match dimension {
            Dimension::D2 => "bvec2(true, true)".to_string(),
            Dimension::D3 => "bvec3(true, true, true)".to_string(),
            Dimension::D4 => "bvec4(true, true, true, true)".to_string(),
        }
    )
}

fn generate_test_in_expression(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Special case for D2 uvec: return bool instead of bvec2 (following manual file)
    if matches!(vec_type, VecType::UVec) && matches!(dimension, Dimension::D2) {
        return format!(
            "bool test_{type_name}_equal_function_in_expression() {{\n\
    {type_name} a = uvec2(1u, 3u);\n\
    {type_name} b = uvec2(1u, 4u);\n\
    {type_name} c = uvec2(2u, 3u);\n\
    return equal(a, b) == equal(b, c);\n\
    // (true,false) == (false,false) = false\n\
}}\n\
\n\
// run: test_{type_name}_equal_function_in_expression() == false\n"
        );
    }

    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [1, 3, 5, 7...], b = [1, 4, 5, 7...], c = [2, 3, 5, 6...]
    // equal(a, b) = [true, false, true, true...]
    // equal(b, c) = [false, false, true, false...]
    // equal(equal(a, b), equal(b, c)) = [false, true, true, false...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 3],
        Dimension::D3 => vec![1, 3, 5],
        Dimension::D4 => vec![1, 3, 5, 7],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 4],
        Dimension::D3 => vec![1, 4, 5],
        Dimension::D4 => vec![1, 4, 5, 7],
    };
    let c_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 3],
        Dimension::D3 => vec![2, 3, 5],
        Dimension::D4 => vec![2, 3, 5, 6],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![false, false],
        Dimension::D3 => vec![false, true, true], // equal((true,false,true), (false,false,true)) = (false,true,true)
        Dimension::D4 => vec![false, true, true, false],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let c_constructor = format_vector_constructor(vec_type, dimension, &c_values);

    format!(
        "{} test_{}_equal_function_in_expression() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         {} c = {};\n\
         // Use equal() for component-wise comparison of {} values\n\
         // equal(a, b) = {}\n\
         // equal(b, c) = {}\n\
         // equal(equal(a, b), equal(b, c)) = {}\n\
         return equal(equal(a, b), equal(b, c));\n\
         }}\n\
         \n\
         // run: test_{}_equal_function_in_expression() == {}\n",
        bvec_type_name,
        type_name,
        type_name,
        a_constructor,
        type_name,
        b_constructor,
        type_name,
        c_constructor,
        bvec_type_name,
        format_bvec_comment(match dimension {
            Dimension::D2 => vec![true, false],
            Dimension::D3 => vec![true, false, true],
            Dimension::D4 => vec![true, false, true, true],
        }),
        format_bvec_comment(match dimension {
            Dimension::D2 => vec![false, false],
            Dimension::D3 => vec![false, false, true],
            Dimension::D4 => vec![false, false, true, false],
        }),
        format_bvec_comment(expected.clone()),
        type_name,
        format_bvec_expected(expected)
    )
}
