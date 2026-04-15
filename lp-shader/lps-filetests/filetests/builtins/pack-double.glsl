// test run

// ============================================================================
// packDouble2x32(): Pack double function
// packDouble2x32(dvec2) - pack 2 doubles to uvec2
// ============================================================================

uvec2 test_packdouble2x32_zeros() {
    // packDouble2x32(dvec2(0.0, 0.0)) should pack to uvec2(0, 0)
    return packDouble2x32(dvec2(0.0, 0.0));
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_packdouble2x32_zeros() == uvec2(0u, 0u)

uvec2 test_packdouble2x32_ones() {
    // packDouble2x32(dvec2(1.0, 1.0)) should pack double precision ones
    return packDouble2x32(dvec2(1.0, 1.0));
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_packdouble2x32_ones() == uvec2(0u, 1072693248u)

uvec2 test_packdouble2x32_half() {
    // packDouble2x32(dvec2(0.5, 0.5)) should pack double precision halves
    return packDouble2x32(dvec2(0.5, 0.5));
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_packdouble2x32_half() == uvec2(0u, 1071644672u)

uvec2 test_packdouble2x32_neg_one() {
    // packDouble2x32(dvec2(-1.0, 1.0)) should pack negative and positive
    return packDouble2x32(dvec2(-1.0, 1.0));
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_packdouble2x32_neg_one() == uvec2(0u, 1072693248u)

uvec2 test_packdouble2x32_two() {
    // packDouble2x32(dvec2(2.0, 2.0)) should pack double precision twos
    return packDouble2x32(dvec2(2.0, 2.0));
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_packdouble2x32_two() == uvec2(0u, 1073741824u)

uvec2 test_packdouble2x32_small() {
    // packDouble2x32(dvec2(0.1, 0.1)) should pack small double values
    return packDouble2x32(dvec2(0.1, 0.1));
}

// @unimplemented(jit.q32)
// @unimplemented(rv32.q32)
// @unimplemented(wasm.q32)
// @unimplemented(rv32fa.q32)
// run: test_packdouble2x32_small() == uvec2(0u, 1069128089u)




