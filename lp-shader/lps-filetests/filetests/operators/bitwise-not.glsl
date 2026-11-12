// test run

// GLSL ES 3.00 §5.9 `~`: one's complement.

int test_not_int() {
    int a = 0;
    return ~a;
}

// run: test_not_int() == -1

int test_not_int_positive() {
    int a = 5;
    return ~a;
}

// run: test_not_int_positive() == -6

uint test_not_uint() {
    uint a = 255u;
    return ~a;
}

// run: test_not_uint() == 4294967040u

ivec2 test_not_ivec2() {
    ivec2 a = ivec2(0, -1);
    return ~a;
}

// run: test_not_ivec2() == ivec2(-1, 0)

uvec2 test_not_uvec2() {
    uvec2 a = uvec2(0u, 4294967295u);
    return ~a;
}

// run: test_not_uvec2() == uvec2(4294967295u, 0u)

int test_not_const_fold() {
    return ~7;
}

// run: test_not_const_fold() == -8

int test_not_twice() {
    int a = 42;
    return ~~a;
}

// run: test_not_twice() == 42
