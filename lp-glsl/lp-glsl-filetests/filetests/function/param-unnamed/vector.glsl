// test run

vec2 combine(vec2 a, vec2 b);

vec2 combine(vec2 a, vec2 b) {
    return a + b;
}

vec2 test_param_unnamed_vector() {
    return combine(vec2(1.0, 2.0), vec2(3.0, 4.0));
}

// run: test_param_unnamed_vector() ~= vec2(4.0, 6.0)
