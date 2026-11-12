// test run

// The compound assignments `&= |= ^= <<= >>=`.

// lps-frontend (naga) rejects every shift ("binary operand kind mismatch":
// naga's GLSL front end makes the shift count unsigned and the lowering wants
// matching operand kinds), so the whole file fails to compile there.

int test_and_assign() {
    int a = 15;
    a &= 6;
    return a;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_and_assign() == 6

int test_or_assign() {
    int a = 8;
    a |= 3;
    return a;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_or_assign() == 11

uint test_xor_assign() {
    uint a = 10u;
    a ^= 3u;
    return a;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_xor_assign() == 9u

int test_shl_assign() {
    int a = 3;
    a <<= 3;
    return a;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shl_assign() == 24

uint test_shr_assign() {
    uint a = 4294967295u;
    a >>= 16;
    return a;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_shr_assign() == 65535u

ivec2 test_vector_assign() {
    ivec2 a = ivec2(12, 7);
    a &= ivec2(4, 3);
    a |= 16;
    a <<= ivec2(1, 2);
    return a;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_vector_assign() == ivec2(40, 76)

int test_component_assign() {
    ivec3 a = ivec3(1, 2, 3);
    a.y ^= 7;
    return a.y;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_component_assign() == 5

uint test_hash_round() {
    // The shape of an integer hash round, the canonical sources' use.
    uint h = 2166136261u;
    h ^= 255u;
    h ^= h >> 16;
    h ^= h << 5;
    return h & 65535u;
}

// @unimplemented(frontend!=lp, backend!=wgpu)
// run: test_hash_round() == 39142u
