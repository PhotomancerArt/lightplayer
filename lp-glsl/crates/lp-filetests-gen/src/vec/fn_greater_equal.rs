//! Generator for fn-greater-equal test files.

use crate::types::{Dimension, VecType};
use crate::util::generate_header;
use crate::vec::util::{
    format_bvec_comment, format_bvec_expected, format_bvec_type_name, format_type_name,
    format_vector_constructor,
};

/// Generate fn-greater-equal test file content.
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Generate header with regeneration command
    let specifier = format!("vec/{type_name}/fn-greater-equal");
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
        "// Greater Than Equal: greaterThanEqual({type_name}, {type_name}) -> {bvec_type_name} (component-wise)\n"
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
    content.push_str(&generate_test_equal(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_mixed_equal(vec_type, dimension));
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

fn generate_test_mixed(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [7, 6, 9, 2...], b = [5, 8, 7, 4...]
    // Result: [true, false, true, false...] (7>=5, 6>=8, 9>=7, 2>=4)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 6],
        Dimension::D3 => vec![7, 6, 9],
        Dimension::D4 => vec![7, 6, 9, 2],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 8],
        Dimension::D3 => vec![5, 8, 7],
        Dimension::D4 => vec![5, 8, 7, 4],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, false],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_greater_equal_mixed() {{\n\
    // Function greaterThanEqual() returns {} (component-wise comparison)\n\
    {} a = {};\n\
    {} b = {};\n\
    return greaterThanEqual(a, b);\n\
}}\n\
\n\
// run: test_{}_greater_equal_mixed() == {}\n",
        bvec_type_name,
        type_name,
        bvec_type_name,
        type_name,
        a_constructor,
        type_name,
        b_constructor,
        type_name,
        format_bvec_expected(expected)
    )
}

fn generate_test_all_true(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 6, 7, 8...], b = [1, 2, 3, 4...]
    // Result: [true, true, true, true...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 6],
        Dimension::D3 => vec![5, 6, 7],
        Dimension::D4 => vec![5, 6, 7, 8],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 2],
        Dimension::D3 => vec![1, 2, 3],
        Dimension::D4 => vec![1, 2, 3, 4],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, true],
        Dimension::D3 => vec![true, true, true],
        Dimension::D4 => vec![true, true, true, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_greater_equal_all_true() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return greaterThanEqual(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_all_true() == {}\n",
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

fn generate_test_all_false(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [1, 2, 3, 4...], b = [5, 6, 7, 8...]
    // Result: [false, false, false, false...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 2],
        Dimension::D3 => vec![1, 2, 3],
        Dimension::D4 => vec![1, 2, 3, 4],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 6],
        Dimension::D3 => vec![5, 6, 7],
        Dimension::D4 => vec![5, 6, 7, 8],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![false, false],
        Dimension::D3 => vec![false, false, false],
        Dimension::D4 => vec![false, false, false, false],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_greater_equal_all_false() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return greaterThanEqual(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_all_false() == {}\n",
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

fn generate_test_equal(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 5, 5, 5...], b = [5, 5, 5, 5...]
    // Result: [true, true, true, true...] (equal values are >=)
    let values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 5],
        Dimension::D3 => vec![5, 5, 5],
        Dimension::D4 => vec![5, 5, 5, 5],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, true],
        Dimension::D3 => vec![true, true, true],
        Dimension::D4 => vec![true, true, true, true],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &values);

    format!(
        "{} test_{}_greater_equal_equal() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return greaterThanEqual(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_equal() == {}\n",
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

fn generate_test_mixed_equal(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 6, 7, 8...], b = [5, 5, 8, 7...]
    // Result: [true, true, false, true...] (5>=5, 6>=5, 7>=8, 8>=7)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 6],
        Dimension::D3 => vec![5, 6, 7],
        Dimension::D4 => vec![5, 6, 7, 8],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 5],
        Dimension::D3 => vec![5, 5, 8],
        Dimension::D4 => vec![5, 5, 8, 7],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, true],
        Dimension::D3 => vec![true, true, false],
        Dimension::D4 => vec![true, true, false, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_greater_equal_mixed_equal() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return greaterThanEqual(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_mixed_equal() == {}\n",
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

    // Values: a = [-1, -3, 2, -5...], b = [-5, -2, 1, -7...]
    // Result: [true, false, true, true...] (-1>=-5, -3>=-2, 2>=1, -5>=-7)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-1, -3],
        Dimension::D3 => vec![-1, -3, 2],
        Dimension::D4 => vec![-1, -3, 2, -5],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-5, -2],
        Dimension::D3 => vec![-5, -2, 1],
        Dimension::D4 => vec![-5, -2, 1, -7],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_greater_equal_negative() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return greaterThanEqual(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_negative() == {}\n",
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

    // For unsigned types, use all non-negative values
    let (a_values, b_values, expected) = if matches!(vec_type, VecType::UVec) {
        // Values: a = [1, 0, 3, 2...], b = [0, 1, 2, 4...]
        // Result: [true, false, true, false...] (1>=0, 0>=1, 3>=2, 2>=4)
        match dimension {
            Dimension::D2 => (vec![1, 0], vec![0, 1], vec![true, false]),
            Dimension::D3 => (vec![1, 0, 3], vec![0, 1, 2], vec![true, false, true]),
            Dimension::D4 => (
                vec![1, 0, 3, 2],
                vec![0, 1, 2, 4],
                vec![true, false, true, false],
            ),
        }
    } else {
        // Values: a = [1, 0, 3, -1...], b = [0, 1, 2, 0...]
        // Result: [true, false, true, false...] (1>=0, 0>=1, 3>=2, -1>=0)
        match dimension {
            Dimension::D2 => (vec![1, 0], vec![0, 1], vec![true, false]),
            Dimension::D3 => (vec![1, 0, 3], vec![0, 1, 2], vec![true, false, true]),
            Dimension::D4 => (
                vec![1, 0, 3, -1],
                vec![0, 1, 2, 0],
                vec![true, false, true, false],
            ),
        }
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_greater_equal_zero() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return greaterThanEqual(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_zero() == {}\n",
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

    // Values: a = [12, 10, 8, 6...], b = [10, 15, 8, 5...]
    // Result: [true, false, true, true...] (12>=10, 10>=15, 8>=8, 6>=5)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![12, 10],
        Dimension::D3 => vec![12, 10, 8],
        Dimension::D4 => vec![12, 10, 8, 6],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![10, 15],
        Dimension::D3 => vec![10, 15, 8],
        Dimension::D4 => vec![10, 15, 8, 5],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_greater_equal_variables() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return greaterThanEqual(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_variables() == {}\n",
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

    // Values: [5, 5, 6, 3...] vs [3, 7, 6, 4...]
    // Result: [true, false, true, false...] (5>=3, 5>=7, 6>=6, 3>=4)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 5],
        Dimension::D3 => vec![5, 5, 6],
        Dimension::D4 => vec![5, 5, 6, 3],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 7],
        Dimension::D3 => vec![3, 7, 6],
        Dimension::D4 => vec![3, 7, 6, 4],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, false],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_greater_equal_expressions() {{\n\
         return greaterThanEqual({}, {});\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_expressions() == {}\n",
        bvec_type_name,
        type_name,
        a_constructor,
        b_constructor,
        type_name,
        format_bvec_expected(expected)
    )
}

fn generate_test_in_expression(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [3, 7, 5, 9...], b = [2, 3, 6, 8...], c = [1, 5, 4, 7...]
    // greaterThanEqual(a, b) = [true, true, false, true...]
    // greaterThanEqual(b, c) = [true, false, true, true...]
    // equal(greaterThanEqual(a, b), greaterThanEqual(b, c)) = [true, false, false, true...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 7],
        Dimension::D3 => vec![3, 7, 5],
        Dimension::D4 => vec![3, 7, 5, 9],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 3],
        Dimension::D3 => vec![2, 3, 6],
        Dimension::D4 => vec![2, 3, 6, 8],
    };
    let c_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 5],
        Dimension::D3 => vec![1, 5, 4],
        Dimension::D4 => vec![1, 5, 4, 7],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, false],
        Dimension::D4 => vec![true, false, false, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let c_constructor = format_vector_constructor(vec_type, dimension, &c_values);

    format!(
        "{} test_{}_greater_equal_in_expression() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         {} c = {};\n\
         // Use equal() for component-wise comparison of {} values\n\
         // greaterThanEqual(a, b) = {}\n\
         // greaterThanEqual(b, c) = {}\n\
         // equal(greaterThanEqual(a, b), greaterThanEqual(b, c)) = {}\n\
         return equal(greaterThanEqual(a, b), greaterThanEqual(b, c));\n\
         }}\n\
         \n\
         // run: test_{}_greater_equal_in_expression() == {}\n",
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
            Dimension::D2 => vec![true, true],
            Dimension::D3 => vec![true, true, false],
            Dimension::D4 => vec![true, true, false, true],
        }),
        format_bvec_comment(match dimension {
            Dimension::D2 => vec![true, false],
            Dimension::D3 => vec![true, false, true],
            Dimension::D4 => vec![true, false, true, true],
        }),
        format_bvec_comment(expected.clone()),
        type_name,
        format_bvec_expected(expected)
    )
}
