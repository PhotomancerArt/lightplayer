//! Generator for fn-greater-than test files.

use crate::types::{Dimension, VecType};
use crate::util::generate_header;
use crate::vec::util::{
    format_bvec_comment, format_bvec_expected, format_bvec_type_name, format_type_name,
    format_vector_constructor,
};

/// Generate fn-less-than test file content.
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Generate header with regeneration command
    let specifier = format!("vec/{type_name}/fn-less-than");
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
        "// Less Than: lessThan({type_name}, {type_name}) -> {bvec_type_name} (component-wise)\n"
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

    // a = [5, 8, 7, 4...], b = [7, 6, 9, 2...]
    // Result: [true, false, true, false...] (5<7, 8<6, 7<9, 4<2)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 8],
        Dimension::D3 => vec![5, 8, 7],
        Dimension::D4 => vec![5, 8, 7, 4],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 6],
        Dimension::D3 => vec![7, 6, 9],
        Dimension::D4 => vec![7, 6, 9, 2],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, false],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_less_than_mixed() {{\n\
         // Function lessThan() returns {} (component-wise comparison)\n\
         {} a = {};\n\
         {} b = {};\n\
         return lessThan(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_less_than_mixed() == {}\n",
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

    // a = [1, 2, 3, 4...], b = [5, 6, 7, 8...]
    // Result: [true, true, true, true...] (1<5, 2<6, 3<7, 4<8)
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
        Dimension::D2 => vec![true, true],
        Dimension::D3 => vec![true, true, true],
        Dimension::D4 => vec![true, true, true, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_less_than_all_true() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return lessThan(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_less_than_all_true() == {}\n",
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

    // a = [5, 6, 7, 8...], b = [1, 2, 3, 4...]
    // Result: [false, false, false, false...] (5<1, 6<2, 7<3, 8<4)
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
        Dimension::D2 => vec![false, false],
        Dimension::D3 => vec![false, false, false],
        Dimension::D4 => vec![false, false, false, false],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_less_than_all_false() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return lessThan(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_less_than_all_false() == {}\n",
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

    // a = [5, 5, 5, 5...], b = [5, 6, 4, 7...]
    // Result: [false, true, false, true...] (5<5=false, 5<6=true, 5<4=false, 5<7=true)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 5],
        Dimension::D3 => vec![5, 5, 5],
        Dimension::D4 => vec![5, 5, 5, 5],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 6],
        Dimension::D3 => vec![5, 6, 4],
        Dimension::D4 => vec![5, 6, 4, 7],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![false, true],
        Dimension::D3 => vec![false, true, false],
        Dimension::D4 => vec![false, true, false, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_less_than_equal() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return lessThan(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_less_than_equal() == {}\n",
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

    // a = [-5, -7, 0, -8...], b = [-1, -3, 2, -5...] (swapped from greaterThan)
    // Result: [true, true, true, true...]
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-5, -7],
        Dimension::D3 => vec![-5, -7, 0],
        Dimension::D4 => vec![-5, -7, 0, -8],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![-1, -3],
        Dimension::D3 => vec![-1, -3, 2],
        Dimension::D4 => vec![-1, -3, 2, -5],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, true],
        Dimension::D3 => vec![true, true, true],
        Dimension::D4 => vec![true, true, true, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_less_than_negative() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return lessThan(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_less_than_negative() == {}\n",
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

    // For unsigned types, avoid negative values in comparisons
    let (a_values, b_values, expected) = if matches!(vec_type, VecType::UVec) {
        // a = [0, 1, 2, 0...], b = [1, 0, 3, 2...]
        // Result: [true, false, true, false...] (0<1, 1<0, 2<3, 0<2)
        match dimension {
            Dimension::D2 => (vec![0, 1], vec![1, 0], vec![true, false]),
            Dimension::D3 => (vec![0, 1, 2], vec![1, 0, 3], vec![true, false, true]),
            Dimension::D4 => (
                vec![0, 1, 2, 0],
                vec![1, 0, 3, 2],
                vec![true, false, true, true],
            ),
        }
    } else {
        // a = [0, 1, 2, 0...], b = [1, 0, 3, -1...]
        // Result: [true, false, true, false...] (0<1, 1<0, 2<3, 0<-1)
        match dimension {
            Dimension::D2 => (vec![0, 1], vec![1, 0], vec![true, false]),
            Dimension::D3 => (vec![0, 1, 2], vec![1, 0, 3], vec![true, false, true]),
            Dimension::D4 => (
                vec![0, 1, 2, 0],
                vec![1, 0, 3, -1],
                vec![true, false, true, false],
            ),
        }
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_less_than_zero() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return lessThan(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_less_than_zero() == {}\n",
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

    // Use the same values as manual files for consistency
    let (a_values, b_values, expected) = match dimension {
        Dimension::D2 => (vec![10, 15], vec![12, 10], vec![true, false]),
        Dimension::D3 => (vec![10, 15, 8], vec![12, 10, 12], vec![true, false, true]),
        Dimension::D4 => (
            vec![10, 15, 8, 12],
            vec![12, 10, 12, 8],
            vec![true, false, true, false],
        ),
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_less_than_variables() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         return lessThan(a, b);\n\
         }}\n\
         \n\
         // run: test_{}_less_than_variables() == {}\n",
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

    // Use the same values as manual files for consistency
    let (a_values, b_values, expected) = match dimension {
        Dimension::D2 => (vec![3, 7], vec![5, 5], vec![true, false]),
        Dimension::D3 => (vec![3, 7, 2], vec![5, 5, 4], vec![true, false, true]),
        Dimension::D4 => (
            vec![3, 7, 2, 9],
            vec![5, 5, 4, 8],
            vec![true, false, true, false],
        ),
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_less_than_expressions() {{\n\
         return lessThan({}, {});\n\
         }}\n\
         \n\
         // run: test_{}_less_than_expressions() == {}\n",
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

    // Special case for D4 uvec: return bool instead of bvec4 (following manual file)
    if matches!(vec_type, VecType::UVec) && matches!(dimension, Dimension::D4) {
        return format!(
            "bool test_{type_name}_less_than_in_expression() {{\n\
    {type_name} a = uvec4(1u, 5u, 3u, 7u);\n\
    {type_name} b = uvec4(2u, 3u, 4u, 5u);\n\
    {type_name} c = uvec4(3u, 7u, 1u, 9u);\n\
    return lessThan(a, b) == lessThan(b, c);\n\
    // (true,false,true,false) == (true,true,false,true) = false\n\
}}\n\
\n\
// run: test_{type_name}_less_than_in_expression() == false\n"
        );
    }

    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // a = [1, 5, 4, 7...], b = [2, 3, 6, 8...], c = [3, 7, 5, 9...]
    // lessThan(a, b) = [true, false, true, true...] (1<2, 5<3, 4<6, 7<8)
    // lessThan(b, c) = [true, true, false, true...] (2<3, 3<7, 6<5, 8<9)
    // equal(lessThan(a, b), lessThan(b, c)) = [true, false, false, true...] (true==true, false==true, true==false, true==true)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 5],
        Dimension::D3 => vec![1, 5, 4],
        Dimension::D4 => vec![1, 5, 4, 7],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 3],
        Dimension::D3 => vec![2, 3, 6],
        Dimension::D4 => vec![2, 3, 6, 8],
    };
    let c_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 7],
        Dimension::D3 => vec![3, 7, 5],
        Dimension::D4 => vec![3, 7, 5, 9],
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
        "{} test_{}_less_than_in_expression() {{\n\
         {} a = {};\n\
         {} b = {};\n\
         {} c = {};\n\
         // Use equal() for component-wise comparison of {} values\n\
         // lessThan(a, b) = {}\n\
         // lessThan(b, c) = {}\n\
         // equal(lessThan(a, b), lessThan(b, c)) = {}\n\
         return equal(lessThan(a, b), lessThan(b, c));\n\
         }}\n\
         \n\
         // run: test_{}_less_than_in_expression() == {}\n",
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
            Dimension::D2 => vec![true, true],
            Dimension::D3 => vec![true, true, false],
            Dimension::D4 => vec![true, true, false, true],
        }),
        format_bvec_comment(expected.clone()),
        type_name,
        format_bvec_expected(expected)
    )
}
