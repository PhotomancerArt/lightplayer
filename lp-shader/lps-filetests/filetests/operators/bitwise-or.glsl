// test run

// GLSL ES 3.00 §5.9 `|`.

int test_or_int() {
    int a = 12;
    int b = 3;
    return a | b;
}

// run: test_or_int() == 15

uint test_or_uint() {
    uint a = 2147483648u;
    return a | 1u;
}

// run: test_or_uint() == 2147483649u

ivec2 test_or_ivec2_scalar() {
    ivec2 a = ivec2(8, 16);
    return a | 1;
}

// run: test_or_ivec2_scalar() == ivec2(9, 17)

uvec4 test_or_uvec4() {
    uvec4 a = uvec4(1u, 2u, 4u, 8u);
    uvec4 b = uvec4(16u, 2u, 1u, 0u);
    return a | b;
}

// run: test_or_uvec4() == uvec4(17u, 2u, 5u, 8u)

int test_or_precedence() {
    // `|` binds looser than `^`, which binds looser than `&`:
    // 1 | 6 ^ 3 & 5 = 1 | (6 ^ (3 & 5)) = 1 | (6 ^ 1) = 1 | 7 = 7
    int a = 1;
    int b = 6;
    int c = 3;
    int d = 5;
    return a | b ^ c & d;
}

// run: test_or_precedence() == 7

int test_or_vs_logical() {
    // `|` and `||` lex apart.
    int a = 4;
    bool t = a > 0 || a < 0;
    return t ? (a | 2) : 0;
}

// run: test_or_vs_logical() == 6
