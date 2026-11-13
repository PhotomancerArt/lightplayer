// test run

// GLSL ES 3.00 §5.9 `^`.

int test_xor_int() {
    int a = 12;
    int b = 10;
    return a ^ b;
}

// run: test_xor_int() == 6

int test_xor_self() {
    int a = 123456;
    return a ^ a;
}

// run: test_xor_self() == 0

uint test_xor_uint() {
    uint a = 4294967295u;
    return a ^ 15u;
}

// run: test_xor_uint() == 4294967280u

uvec3 test_xor_uvec3() {
    uvec3 a = uvec3(1u, 2u, 3u);
    uvec3 b = uvec3(3u, 3u, 3u);
    return a ^ b;
}

// run: test_xor_uvec3() == uvec3(2u, 1u, 0u)

bool test_xor_vs_logical() {
    // `^` and `^^` lex apart.
    bool p = true;
    bool q = false;
    int x = 5 ^ 1;
    return (p ^^ q) && x == 4;
}

// run: test_xor_vs_logical() == true
