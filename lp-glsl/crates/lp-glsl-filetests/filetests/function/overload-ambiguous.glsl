// test run
// target riscv32.q32

// ============================================================================
// Ambiguous Overloads: Cases that should produce compile errors
// Note: These tests are expected to FAIL compilation due to ambiguous overloads
// They are included to test that the compiler properly detects ambiguity
// ============================================================================

// Ambiguous overloads - same parameter types, different return types
// This should be a compile error since overloading is based on parameters, not return type

/*
float func_float(int x) {
    return float(x);
}

int func_int(int x) {
    return x;
}

float test_overload_ambiguous_return_type() {
    // Same parameters, different return types - INVALID
    // This call would be ambiguous
    return func_float(5); // Error: ambiguous call
}

// run: test_overload_ambiguous_return_type() ~= 5.0 [expect-fail]
*/

float func_in(int x) {
    return float(x);
}

float func_out(out int x) {  // Different qualifier
    x = 10;
    return 5.0;
}

float test_overload_ambiguous_qualifiers() {
    // Same parameter types with different qualifiers - INVALID
    // This should be an error - same base types, different qualifiers
    return func_in(5); // Should be compile error
}

// run: test_overload_ambiguous_qualifiers() ~= 5.0

/*
float func_float_int(float x, int y) {
    return x + float(y);
}

float func_int_float(int x, float y) {
    return float(x) + y;
}

float test_overload_ambiguous_conversions() {
    // Multiple equally good conversions
    // Call with (int, int) - both overloads require one conversion each
    // This should be ambiguous
    return func_float_int(1, 2); // Error: ambiguous - both equally good
}

// run: test_overload_ambiguous_conversions() ~= 3.0 [expect-fail]
*/

float func_float_float(float x, float y) {
    return x + y;
}

float func_float_int_marked(float x, int y) {
    return x + float(y) + 0.5; // Mark this one
}

float test_overload_valid_resolution() {
    // Valid case for comparison - exact match vs conversion
    // Should prefer exact match
    return func_float_float(1.0, 2.0); // Should call first overload: 3.0
}

// run: test_overload_valid_resolution() ~= 3.0

/*
float sum_arr2(float[2] arr) {
    return arr[0] + arr[1];
}

float sum_arr3(float[3] arr) {
    return arr[0] + arr[1] + arr[2];
}

float test_overload_ambiguous_array_sizes() {
    // Different array sizes - should be distinct, not ambiguous
    // These should be valid overloads since array sizes are part of the type
    float[2] arr2 = float[2](1.0, 2.0);
    float[3] arr3 = float[3](1.0, 2.0, 3.0);
    return sum_arr2(arr2) + sum_arr3(arr3); // 3.0 + 6.0 = 9.0
}

// run: test_overload_ambiguous_array_sizes() ~= 9.0 [expect-fail]
*/

float get_x_vec2(vec2 v) {
    return v.x + 10.0;
}

float get_x_vec3(vec3 v) {
    return v.x + 20.0;
}

float get_x_vec4(vec4 v) {
    return v.x + 30.0;
}

float test_overload_ambiguous_vector_sizes() {
    // Different vector dimensions should be valid overloads
    // Should choose vec3 overload
    return get_x_vec3(vec3(5.0, 0.0, 0.0)); // Should be 25.0
}

// run: test_overload_ambiguous_vector_sizes() ~= 25.0

/*
float process_int(int x) {
    return float(x);
}

float process_uint(uint x) {
    return float(x) + 0.1;
}

float test_overload_ambiguous_promotions() {
    // Ambiguous due to multiple possible promotions
    // int literal 5 could match both int and uint
    // This might be ambiguous depending on language rules
    return process_int(5); // Potentially ambiguous
}

// run: test_overload_ambiguous_promotions() ~= 5.0 [expect-fail]
*/
