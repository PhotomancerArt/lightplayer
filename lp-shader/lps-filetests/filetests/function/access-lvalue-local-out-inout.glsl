// test run
// Focused M9 phase 2: local vector/matrix access-shaped `out` / `inout` actuals (temp + writeback).

void set_scalar(out float x) {
    x = 21.0;
}

float test_access_index_vector_lane_out() {
    vec2 v = vec2(0.0, 0.0);
    set_scalar(v[1]);
    return v[1];
}

// run: test_access_index_vector_lane_out() ~= 21.0

void bump(inout float x) {
    x = x + 1.0;
}

float test_access_index_vector_lane_inout() {
    vec3 v = vec3(1.0, 2.0, 3.0);
    bump(v[0]);
    return v[0];
}

// run: test_access_index_vector_lane_inout() ~= 2.0

void set_two(out vec2 c) {
    c = vec2(3.0, 4.0);
}

float test_access_index_matrix_column_out() {
    mat2 m = mat2(1.0, 0.0, 0.0, 1.0);
    set_two(m[1]);
    return m[1].x + m[1].y;
}

// run: test_access_index_matrix_column_out() ~= 7.0

void scale_cell(inout float x) {
    x = x * 10.0;
}

float test_access_index_matrix_cell_inout() {
    // Column-major: col0 (1,2), col1 (3,4), so m[0][1] == 2.0
    mat2 m = mat2(1.0, 2.0, 3.0, 4.0);
    scale_cell(m[0][1]);
    return m[0][1];
}

// run: test_access_index_matrix_cell_inout() ~= 20.0

// Phase 3: local aggregate access (array / struct / nested struct / array-of-struct).

void fill_nine(out float x) {
    x = 9.0;
}

float test_array_const_element_out() {
    float a[3];
    a[0] = 0.0;
    a[1] = 0.0;
    a[2] = 0.0;
    fill_nine(a[1]);
    return a[1];
}

// run: test_array_const_element_out() ~= 9.0

float test_array_dynamic_element_inout() {
    float a[3];
    a[0] = 1.0;
    a[1] = 2.0;
    a[2] = 4.0;
    int i = 2;
    bump(a[i]);
    return a[2];
}

// run: test_array_dynamic_element_inout() ~= 5.0

struct SFloat {
    float f;
};

void set_seven(out float x) {
    x = 7.0;
}

float test_struct_scalar_field_out() {
    SFloat s = SFloat(0.0);
    set_seven(s.f);
    return s.f;
}

// run: test_struct_scalar_field_out() ~= 7.0

struct InnerN {
    float value;
};

struct OuterN {
    InnerN inner;
};

float test_nested_inner_value_inout() {
    OuterN o = OuterN(InnerN(3.0));
    bump(o.inner.value);
    return o.inner.value;
}

// run: test_nested_inner_value_inout() ~= 4.0

struct Point2 {
    float x;
    float y;
};

float test_array_of_struct_member_inout() {
    Point2 ps[2];
    ps[0] = Point2(1.0, 2.0);
    ps[1] = Point2(3.0, 4.0);
    int i = 0;
    scale_cell(ps[i].x);
    return ps[0].x;
}

// run: test_array_of_struct_member_inout() ~= 10.0

// Phase 4: writable access actuals rooted in `out` / `inout` pointer parameters.

void set_float(out float x) {
    x = 7.0;
}

void wrapper_array(inout float arr[2]) {
    set_float(arr[1]);
}

float test_pointer_arg_array_element_out() {
    float a[2];
    a[0] = 1.0;
    a[1] = 2.0;
    wrapper_array(a);
    return a[1];
}

// run: test_pointer_arg_array_element_out() ~= 7.0

void wrapper_vec(inout vec3 v) {
    set_float(v.y);
}

float test_pointer_arg_vector_lane_out() {
    vec3 v = vec3(1.0, 2.0, 3.0);
    wrapper_vec(v);
    return v.y;
}

// run: test_pointer_arg_vector_lane_out() ~= 7.0

struct Inner {
    float value;
};

struct Outer {
    Inner inner;
};

void wrapper_struct(inout Outer o) {
    set_float(o.inner.value);
}

float test_pointer_arg_nested_struct_field_out() {
    Outer o = Outer(Inner(0.0));
    wrapper_struct(o);
    return o.inner.value;
}

// run: test_pointer_arg_nested_struct_field_out() ~= 7.0
