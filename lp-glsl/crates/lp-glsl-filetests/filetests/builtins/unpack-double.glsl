// test run
// target riscv32.q32

// ============================================================================
// unpackDouble2x32(): Unpack double function
// unpackDouble2x32(uvec2) - unpack uvec2 to dvec2
// ============================================================================

dvec2 test_unpackdouble2x32_zero() {
    // unpackDouble2x32(uvec2(0, 0)) should unpack to dvec2(0.0, 0.0)
    return unpackDouble2x32(uvec2(0u, 0u));
}

// run: test_unpackdouble2x32_zero() ~= dvec2(0.0, 0.0) [expect-fail]

dvec2 test_unpackdouble2x32_ones() {
    // unpackDouble2x32 double precision ones
    return unpackDouble2x32(uvec2(0u, 1072693248u));
}

// run: test_unpackdouble2x32_ones() ~= dvec2(1.0, 1.0) [expect-fail]

dvec2 test_unpackdouble2x32_half() {
    // unpackDouble2x32 double precision halves
    return unpackDouble2x32(uvec2(0u, 1071644672u));
}

// run: test_unpackdouble2x32_half() ~= dvec2(0.5, 0.5) [expect-fail]

dvec2 test_unpackdouble2x32_neg_one() {
    // unpackDouble2x32 negative and positive
    return unpackDouble2x32(uvec2(0u, 1072693248u));
}

// run: test_unpackdouble2x32_neg_one() ~= dvec2(-1.0, 1.0) [expect-fail]

dvec2 test_unpackdouble2x32_two() {
    // unpackDouble2x32 double precision twos
    return unpackDouble2x32(uvec2(0u, 1073741824u));
}

// run: test_unpackdouble2x32_two() ~= dvec2(2.0, 2.0) [expect-fail]

dvec2 test_unpackdouble2x32_small() {
    // unpackDouble2x32 small double values
    return unpackDouble2x32(uvec2(0u, 1069128089u));
}

// run: test_unpackdouble2x32_small() ~= dvec2(0.1, 0.1) [expect-fail]




