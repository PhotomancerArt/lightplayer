// test run

// ============================================================================
// Local Variable Scope: Variables declared in functions
// ============================================================================

float global_value = 100.0;

float local_func() {
    float local_var = 42.0;
    return local_var;
}

float test_scope_local_simple() {
    // Local variables are scoped to function
    return local_func();
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_simple() ~= 42.0

float access_global() {
    float global_value = 200.0; // Shadows global
    return global_value;
}

float test_scope_local_shadow_global() {
    // Local variables shadow globals
    return access_global();
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_shadow_global() ~= 200.0

float process_locals() {
    float a = 1.0;
    float b = 2.0;
    float c = a + b;
    return c;
}

float test_scope_local_multiple() {
    // Multiple local variables
    return process_locals();
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_multiple() ~= 3.0

float sum_loop(int n) {
    float sum = 0.0;
    for (int i = 0; i < n; i++) {
        float local_i = float(i);
        sum = sum + local_i;
    }
    return sum;
}

float test_scope_local_in_loop() {
    // Local variables in loops
    return sum_loop(5);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_in_loop() ~= 10.0

float inner_func() {
    float inner_var = 20.0;
    return inner_var;
}

float outer_func() {
    float outer_var = 10.0;
    return outer_var + inner_func(); // Can call other functions
}

float test_scope_local_nested() {
    // Nested function scopes (via function calls)
    return outer_func();
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_nested() ~= 30.0

float use_params(float param1, float param2) {
    float local_calc = param1 * 2.0 + param2 * 3.0;
    return local_calc;
}

float test_scope_local_parameters() {
    // Parameters are also local to function
    return use_params(2.0, 3.0);
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_parameters() ~= 13.0

float mixed_types() {
    int int_var = 5;
    float float_var = 3.14;
    bool bool_var = true;
    vec2 vec_var = vec2(1.0, 2.0);

    return float(int_var) + float_var + (bool_var ? 1.0 : 0.0) + vec_var.x + vec_var.y;
}

float test_scope_local_types() {
    // Different types of local variables
    return mixed_types();
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_types() ~= 12.14

float sum_local_array() {
    float[3] local_arr = float[3](1.0, 2.0, 3.0);
    return local_arr[0] + local_arr[1] + local_arr[2];
}

float test_scope_local_arrays() {
    // Local arrays
    return sum_local_array();
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_arrays() ~= 6.0

struct LocalStruct {
    float x, y;
};

LocalStruct create_local_struct() {
    LocalStruct s = LocalStruct(5.0, 10.0);
    return s;
}

LocalStruct test_scope_local_struct() {
    // Local structs
    return create_local_struct();
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_struct() ~= LocalStruct(5.0, 10.0)

float modify_local() {
    float value = 5.0;
    value = value * 2.0;
    value = value + 3.0;
    return value;
}

float test_scope_local_modification() {
    // Local variables can be modified
    return modify_local();
}

// @unimplemented(jit.q32)
// @unimplemented(rv32c.q32)
// @unimplemented(rv32n.q32)
// @unimplemented(wasm.q32)
// run: test_scope_local_modification() ~= 13.0
