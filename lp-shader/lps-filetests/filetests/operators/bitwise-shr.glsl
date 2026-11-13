// test run

// GLSL ES 3.00 §5.9 `>>`: sign-extending on int, zero-filling on uint.

// lps-frontend (naga) rejects every shift ("binary operand kind mismatch":
// naga's GLSL front end makes the shift count unsigned and the lowering wants
// matching operand kinds), so the whole file fails to compile there.

int test_shr_int() {
    int a = 48;
    return a >> 4;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_int() == 3

int test_shr_int_negative() {
    int a = -16;
    return a >> 2;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_int_negative() == -4

uint test_shr_uint_high_bit() {
    uint a = 2147483648u;
    return a >> 31;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_uint_high_bit() == 1u

uint test_shr_uint_uint_count() {
    uint a = 4294967295u;
    uint n = 28u;
    return a >> n;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_uint_uint_count() == 15u

ivec2 test_shr_ivec2() {
    ivec2 a = ivec2(-8, 64);
    ivec2 n = ivec2(1, 3);
    return a >> n;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_ivec2() == ivec2(-4, 8)

uvec3 test_shr_uvec3_scalar() {
    uvec3 a = uvec3(256u, 16u, 4294967295u);
    return a >> 4u;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_uvec3_scalar() == uvec3(16u, 1u, 268435455u)

bool test_shr_vs_greater() {
    // `>>` and `>` / `>=` lex apart.
    int a = 64 >> 3;
    return a > 7 && a >= 8;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_vs_greater() == true

int test_shr_const_fold() {
    return -32 >> 3;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_const_fold() == -4
