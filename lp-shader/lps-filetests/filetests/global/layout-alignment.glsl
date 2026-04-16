// test run

// ============================================================================
// Layout: Mixed-type globals verify std430 alignment is correct
// ============================================================================
// Declarations in different orders exercise alignment padding.

float ga = 1.0;
vec2 gv2 = vec2(2.0, 3.0);
float gb = 4.0;
vec4 gv4 = vec4(5.0, 6.0, 7.0, 8.0);

float test_layout_read_all() {
    return ga + gv2.x + gv2.y + gb + gv4.x + gv4.y + gv4.z + gv4.w;
}

// 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 = 36
// run: test_layout_read_all() ~= 36.0

float test_layout_mutate_first() {
    ga = 100.0;
    return ga + gv2.x;
}

// run: test_layout_mutate_first() ~= 102.0

vec4 test_layout_read_vec4() {
    return gv4;
}

// run: test_layout_read_vec4() ~= vec4(5.0, 6.0, 7.0, 8.0)
