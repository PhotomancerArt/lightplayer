// test run
// Phase 5: writable `out` / `inout` actuals rooted in private globals (VMContext).

float g_scalar = 0.0;
vec2 g_vec = vec2(0.0, 0.0);
float g_arr[3];

struct GSFloat {
    float f;
};

GSFloat g_struct = GSFloat(0.0);

struct GInner {
    float value;
};

struct GOuter {
    GInner inner;
};

GOuter g_nested = GOuter(GInner(0.0));

mat2 g_mat = mat2(1.0);

void set_scalar(out float x) {
    x = 21.0;
}

float test_global_scalar_out_actual() {
    g_scalar = 0.0;
    set_scalar(g_scalar);
    return g_scalar;
}

// run: test_global_scalar_out_actual() ~= 21.0

void bump(inout float x) {
    x = x + 1.0;
}

float test_global_scalar_inout() {
    g_scalar = 5.0;
    bump(g_scalar);
    return g_scalar;
}

// run: test_global_scalar_inout() ~= 6.0

float test_global_vec_lane_out() {
    g_vec = vec2(0.0, 0.0);
    set_scalar(g_vec.y);
    return g_vec.y;
}

// run: test_global_vec_lane_out() ~= 21.0

float test_global_array_elem_out() {
    g_arr[0] = 0.0;
    g_arr[1] = 0.0;
    g_arr[2] = 0.0;
    set_scalar(g_arr[1]);
    return g_arr[1];
}

// run: test_global_array_elem_out() ~= 21.0

float test_global_struct_field_out() {
    g_struct.f = 0.0;
    set_scalar(g_struct.f);
    return g_struct.f;
}

// run: test_global_struct_field_out() ~= 21.0

float test_global_nested_struct_field_out() {
    g_nested.inner.value = 0.0;
    set_scalar(g_nested.inner.value);
    return g_nested.inner.value;
}

// run: test_global_nested_struct_field_out() ~= 21.0

void set_two(out vec2 c) {
    c = vec2(3.0, 4.0);
}

float test_global_matrix_column_out() {
    g_mat = mat2(1.0, 0.0, 0.0, 1.0);
    set_two(g_mat[1]);
    return g_mat[1].x + g_mat[1].y;
}

// run: test_global_matrix_column_out() ~= 7.0
