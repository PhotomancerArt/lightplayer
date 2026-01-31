//! Unit tests for format_glsl_value() matrix display formatting

use lp_glsl_compiler::GlslValue;
use lp_glsl_filetests::util::format_glsl_value;

#[test]
fn test_format_mat2x2() {
    // mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))
    // Internal representation: [[1.0, 2.0], [3.0, 4.0]] (m[col][row])
    let mat = GlslValue::Mat2x2([
        [1.0, 2.0], // col 0: row0=1.0, row1=2.0
        [3.0, 4.0], // col 1: row0=3.0, row1=4.0
    ]);

    let formatted = format_glsl_value(&mat);
    // Should display as: mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))
    // Column 0: [1.0, 2.0], Column 1: [3.0, 4.0]
    assert_eq!(formatted, "mat2(vec2(1.0, 2.0), vec2(3.0, 4.0))");

    // Verify it's valid GLSL that can be parsed
    let parsed = GlslValue::parse(&formatted).unwrap();
    match (mat, parsed) {
        (GlslValue::Mat2x2(m1), GlslValue::Mat2x2(m2)) => {
            assert_eq!(m1, m2);
        }
        _ => panic!("Parsed value should match original"),
    }
}

#[test]
fn test_format_mat3x3() {
    // mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0))
    // Column 0: [1.0, 2.0, 3.0]
    // Column 1: [4.0, 5.0, 6.0]
    // Column 2: [7.0, 8.0, 9.0]
    // Internal representation (column-major): [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]]
    let mat = GlslValue::Mat3x3([
        [1.0, 2.0, 3.0], // col 0: row0=1.0, row1=2.0, row2=3.0
        [4.0, 5.0, 6.0], // col 1: row0=4.0, row1=5.0, row2=6.0
        [7.0, 8.0, 9.0], // col 2: row0=7.0, row1=8.0, row2=9.0
    ]);

    let formatted = format_glsl_value(&mat);
    // Should display as: mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0))
    // This verifies column-major order: column 0 = [1.0, 2.0, 3.0], column 1 = [4.0, 5.0, 6.0], column 2 = [7.0, 8.0, 9.0]
    assert_eq!(
        formatted,
        "mat3(vec3(1.0, 2.0, 3.0), vec3(4.0, 5.0, 6.0), vec3(7.0, 8.0, 9.0))"
    );

    // Verify it's valid GLSL that can be parsed
    let parsed = GlslValue::parse(&formatted).unwrap();
    match (mat, parsed) {
        (GlslValue::Mat3x3(m1), GlslValue::Mat3x3(m2)) => {
            assert_eq!(m1, m2);
        }
        _ => panic!("Parsed value should match original"),
    }
}

#[test]
fn test_format_mat4x4() {
    // mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec4(13.0, 14.0, 15.0, 16.0))
    // Column 0: [1.0, 2.0, 3.0, 4.0]
    // Column 1: [5.0, 6.0, 7.0, 8.0]
    // Column 2: [9.0, 10.0, 11.0, 12.0]
    // Column 3: [13.0, 14.0, 15.0, 16.0]
    // Internal representation (column-major):
    let mat = GlslValue::Mat4x4([
        [1.0, 2.0, 3.0, 4.0],     // col 0: row0=1.0, row1=2.0, row2=3.0, row3=4.0
        [5.0, 6.0, 7.0, 8.0],     // col 1: row0=5.0, row1=6.0, row2=7.0, row3=8.0
        [9.0, 10.0, 11.0, 12.0],  // col 2: row0=9.0, row1=10.0, row2=11.0, row3=12.0
        [13.0, 14.0, 15.0, 16.0], // col 3: row0=13.0, row1=14.0, row2=15.0, row3=16.0
    ]);

    let formatted = format_glsl_value(&mat);
    // Should display as: mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec4(13.0, 14.0, 15.0, 16.0))
    // This verifies column-major order
    assert_eq!(
        formatted,
        "mat4(vec4(1.0, 2.0, 3.0, 4.0), vec4(5.0, 6.0, 7.0, 8.0), vec4(9.0, 10.0, 11.0, 12.0), vec4(13.0, 14.0, 15.0, 16.0))"
    );

    // Verify it's valid GLSL that can be parsed
    let parsed = GlslValue::parse(&formatted).unwrap();
    match (mat, parsed) {
        (GlslValue::Mat4x4(m1), GlslValue::Mat4x4(m2)) => {
            assert_eq!(m1, m2);
        }
        _ => panic!("Parsed value should match original"),
    }
}

#[test]
fn test_format_mat2x2_with_negative_values() {
    let mat = GlslValue::Mat2x2([
        [-1.0, 2.0], // col 0: row0=-1.0, row1=2.0
        [3.0, -4.0], // col 1: row0=3.0, row1=-4.0
    ]);

    let formatted = format_glsl_value(&mat);
    assert_eq!(formatted, "mat2(vec2(-1.0, 2.0), vec2(3.0, -4.0))");
}
