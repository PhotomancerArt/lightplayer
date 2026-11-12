// test error

// GLSL defines no arithmetic over bool / bvecN (spec 5.9 "Expressions": the
// arithmetic operators operate on float, int and uint only) and no implicit
// conversion out of bool. `lps-glsl` accepted `bvec2 + bvec2` for a while
// because its operand-join has `Bool` as the neutral element, so rv32n.q32
// ran a shader every other target rejected with "unsupported bool binary Add".

bool test_bvec_add() {
    bvec2 a = bvec2(true, false);
    bvec2 b = bvec2(false, true);
    bvec2 c = a + b;  // expected-error {{'+' expects numeric lanes, got BVec2 and BVec2}}
    return c[0];
}
