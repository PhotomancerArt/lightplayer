// test run

// ============================================================================
// Edge cases: single-member, large structs, padding, consecutive same-type, RMW
// ============================================================================

struct Point {
    float x;
    float y;
};

// --- 1) Single-member structs ---

struct SingleFloat {
    float value;
};

struct SingleVec3 {
    vec3 v;
};

struct SingleNested {
    Point p;
};

// --- 2) Many members (register / slot pressure) ---

struct BigStruct {
    float f1, f2, f3, f4, f5, f6, f7, f8;
    float f9, f10, f11, f12, f13, f14, f15, f16;
};

// --- 3) Alignment / padding (std430-style vec3 + scalar) ---

struct PaddingTest1 {
    vec3 v;  // 12 bytes; struct often padded/aligned
    float f;
};

struct PaddingTest2 {
    float f; // 4 bytes
    vec3 v;  // 12 bytes (aligned)
};

// --- 4) Same-type consecutive members ---

struct Floats4 {
    float a, b, c, d;
};

struct Vec2s2 {
    vec2 a, b;
};

// --- Tests ---

float test_single_member_float() {
    SingleFloat s = SingleFloat(42.0);
    return s.value;
}

// @unimplemented(jit.q32)
// run: test_single_member_float() ~= 42.0

float test_single_member_vec3_sum() {
    SingleVec3 s = SingleVec3(vec3(1.0, 2.0, 3.0));
    return s.v.x + s.v.y + s.v.z;
}

// @unimplemented(jit.q32)
// run: test_single_member_vec3_sum() ~= 6.0

float test_single_member_nested() {
    SingleNested s = SingleNested(Point(3.0, 4.0));
    return s.p.x + s.p.y;
}

// @unimplemented(jit.q32)
// run: test_single_member_nested() ~= 7.0

float test_big_struct_16_fields() {
    BigStruct b = BigStruct(
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0
    );
    return b.f1 + b.f16;
}

// @unimplemented(jit.q32)
// run: test_big_struct_16_fields() ~= 17.0

float test_big_struct_f8_f9() {
    BigStruct b = BigStruct(
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0
    );
    return b.f8 + b.f9;
}

// @unimplemented(jit.q32)
// run: test_big_struct_f8_f9() ~= 17.0

float test_big_struct_f2_f15() {
    BigStruct b = BigStruct(
        1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0,
        9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0
    );
    return b.f2 + b.f15;
}

// @unimplemented(jit.q32)
// run: test_big_struct_f2_f15() ~= 17.0

float test_padding1_members() {
    PaddingTest1 p = PaddingTest1(vec3(1.0, 2.0, 3.0), 4.0);
    return p.v.x + p.v.y + p.v.z + p.f;
}

// @unimplemented(jit.q32)
// run: test_padding1_members() ~= 10.0

float test_padding2_members() {
    PaddingTest2 p = PaddingTest2(1.0, vec3(2.0, 3.0, 4.0));
    return p.f + p.v.x + p.v.y - p.v.z; // 1 + 2 + 3 - 4
}

// @unimplemented(jit.q32)
// run: test_padding2_members() ~= 2.0

float test_floats4_sum() {
    Floats4 f = Floats4(1.0, 2.0, 3.0, 4.0);
    return f.a + f.b + f.c + f.d;
}

// @unimplemented(jit.q32)
// run: test_floats4_sum() ~= 10.0

float test_floats4_reassign() {
    Floats4 f = Floats4(1.0, 2.0, 3.0, 4.0);
    f.c = 10.0;
    return f.a + f.b + f.c + f.d; // 1+2+10+4
}

// @unimplemented(jit.q32)
// run: test_floats4_reassign() ~= 17.0

float test_vec2s2_sums() {
    Vec2s2 v = Vec2s2(vec2(1.0, 2.0), vec2(3.0, 4.0));
    return v.a.x + v.a.y + v.b.x + v.b.y;
}

// @unimplemented(jit.q32)
// run: test_vec2s2_sums() ~= 10.0

float test_vec2s2_b_x() {
    Vec2s2 v = Vec2s2(vec2(0.0, 0.0), vec2(5.0, 6.0));
    return v.b.x;
}

// @unimplemented(jit.q32)
// run: test_vec2s2_b_x() ~= 5.0

float test_rmw_sequence() {
    Point p = Point(1.0, 2.0);
    float a = p.x; // read
    p.x = 5.0;     // modify
    float b = p.x; // read
    return a + b;  // 1.0 + 5.0
}

// @unimplemented(jit.q32)
// run: test_rmw_sequence() ~= 6.0

float test_rmw_sequence_y() {
    Point p = Point(0.0, 3.0);
    float a = p.y;
    p.y = 7.0;
    float b = p.y;
    return a + b; // 3 + 7
}

// @unimplemented(jit.q32)
// run: test_rmw_sequence_y() ~= 10.0

float test_rmw_padding1_float() {
    PaddingTest1 p = PaddingTest1(vec3(0.0, 0.0, 0.0), 1.0);
    float a = p.f;
    p.f = 9.0;
    float b = p.f;
    return a + b;
}

// @unimplemented(jit.q32)
// run: test_rmw_padding1_float() ~= 10.0

float test_padding1_whole_copy() {
    PaddingTest1 a = PaddingTest1(vec3(1.0, 2.0, 3.0), 4.0);
    PaddingTest1 b = a;
    return b.v.x + b.f;
}

// @unimplemented(jit.q32)
// run: test_padding1_whole_copy() ~= 5.0

float test_single_vec3_dot() {
    SingleVec3 s = SingleVec3(vec3(2.0, 0.0, 0.0));
    return dot(s.v, s.v);
}

// @unimplemented(jit.q32)
// run: test_single_vec3_dot() ~= 4.0

float test_single_nested_rmw() {
    SingleNested s = SingleNested(Point(1.0, 2.0));
    float a = s.p.x;
    s.p.x = 4.0;
    return a + s.p.x;
}

// @unimplemented(jit.q32)
// run: test_single_nested_rmw() ~= 5.0
