// test run
// target riscv32.q32

// ============================================================================
// Edge Multiple Qualifiers Error: Multiple storage qualifiers are not allowed
// ============================================================================

// Valid declarations with single qualifiers
const float valid_const = 3.14;
uniform float valid_uniform;
in vec2 valid_in;
out vec3 valid_out;
buffer DataBlock { float valid_buffer; };

// These would be compile errors (multiple storage qualifiers):
// const uniform float bad_const_uniform = 1.0;  // Error: const and uniform
// in out vec4 bad_in_out;                        // Error: in and out
// const in float bad_const_in = 2.0;            // Error: const and in
// uniform out mat4 bad_uniform_out;             // Error: uniform and out
// buffer shared float bad_buffer_shared;        // Error: buffer and shared

// Test that single qualifiers work correctly
float test_edge_multiple_qualifiers_error_const() {
    // Single const qualifier works
    return valid_const * 2.0;
}

// run: test_edge_multiple_qualifiers_error_const() ~= 6.28 [expect-fail]

float test_edge_multiple_qualifiers_error_uniform() {
    // Single uniform qualifier works
    return valid_uniform + 1.0;
}

// run: test_edge_multiple_qualifiers_error_uniform() ~= 1.0 [expect-fail]

vec2 test_edge_multiple_qualifiers_error_in() {
    // Single in qualifier works
    return valid_in + vec2(1.0, 1.0);
}

// run: test_edge_multiple_qualifiers_error_in() ~= vec2(1.0, 1.0) [expect-fail]

void test_edge_multiple_qualifiers_error_out() {
    // Single out qualifier works
    valid_out = vec3(1.0, 0.0, 0.0);
}

// run: test_edge_multiple_qualifiers_error_out() == 0.0 [expect-fail]

float test_edge_multiple_qualifiers_error_buffer() {
    // Single buffer qualifier works
    valid_buffer = 42.0;
    return valid_buffer;
}

// run: test_edge_multiple_qualifiers_error_buffer() ~= 42.0 [expect-fail]

float test_edge_multiple_qualifiers_error_combined() {
    // Combined use of properly qualified globals
    float result = valid_const;
    result = result + valid_uniform;
    result = result + valid_in.x + valid_in.y;

    valid_buffer = result;
    result = result + valid_buffer;

    return result;
}

// run: test_edge_multiple_qualifiers_error_combined() ~= 8.28 [expect-fail]
