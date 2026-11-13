// test run

// GLSL ES 3.00 §5.9 `&`: int/uint scalars and vectors, component-wise, a
// scalar operand broadcast over a vector.

int test_and_int() {
    int a = 12; // 0b1100
    int b = 10; // 0b1010
    return a & b;
}

// run: test_and_int() == 8

int test_and_int_negative() {
    int a = -1;
    int b = 7;
    return a & b;
}

// run: test_and_int_negative() == 7

uint test_and_uint() {
    uint a = 4294967295u;
    uint b = 255u;
    return a & b;
}

// run: test_and_uint() == 255u

ivec3 test_and_ivec3() {
    ivec3 a = ivec3(15, 6, 255);
    ivec3 b = ivec3(9, 3, 16);
    return a & b;
}

// run: test_and_ivec3() == ivec3(9, 2, 16)

uvec2 test_and_uvec2_scalar() {
    uvec2 a = uvec2(13u, 6u);
    return a & 5u;
}

// run: test_and_uvec2_scalar() == uvec2(5u, 4u)

int test_and_const_fold() {
    return 12 & 10;
}

// run: test_and_const_fold() == 8

bool test_and_precedence() {
    // `&` binds looser than `==`, so the comparison needs the parentheses.
    int a = 6;
    return (a & 2) == 2;
}

// run: test_and_precedence() == true
