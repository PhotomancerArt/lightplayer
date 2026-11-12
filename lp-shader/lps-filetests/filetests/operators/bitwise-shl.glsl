// test run

// GLSL ES 3.00 §5.9 `<<`: the result has the left operand's type; the shift
// count is int or uint, scalar or of the same width.

int test_shl_int() {
    int a = 3;
    return a << 4;
}

// run: test_shl_int() == 48

int test_shl_into_sign() {
    int a = 1;
    return a << 31;
}

// run: test_shl_into_sign() == -2147483648

uint test_shl_uint_int_count() {
    uint a = 1u;
    int n = 31;
    return a << n;
}

// run: test_shl_uint_int_count() == 2147483648u

ivec3 test_shl_ivec3_scalar() {
    ivec3 a = ivec3(1, 2, 3);
    return a << 2;
}

// run: test_shl_ivec3_scalar() == ivec3(4, 8, 12)

uvec2 test_shl_uvec2_ivec2() {
    uvec2 a = uvec2(1u, 1u);
    ivec2 n = ivec2(3, 8);
    return a << n;
}

// run: test_shl_uvec2_ivec2() == uvec2(8u, 256u)

int test_shl_precedence() {
    // `<<` binds looser than `+`: 1 << 2 + 1 = 1 << 3
    int a = 1;
    return a << 2 + 1;
}

// run: test_shl_precedence() == 8

bool test_shl_vs_less() {
    // `<<` and `<` / `<=` lex apart.
    int a = 1 << 2;
    return a < 5 && a <= 4;
}

// run: test_shl_vs_less() == true

int test_shl_const_fold() {
    return 5 << 3;
}

// run: test_shl_const_fold() == 40
