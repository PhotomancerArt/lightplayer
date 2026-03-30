// test run

// ============================================================================
// isinf(): Is infinity function (Q32: always false — docs/design/q32.md §6)
// ============================================================================

bool test_isinf_normal() {
    return isinf(1.0);
}

// run: test_isinf_normal() == false

bool test_isinf_zero() {
    return isinf(0.0);
}

// run: test_isinf_zero() == false

bool test_isinf_inf() {
    float a = 1.0;
    float b = 0.0;
    return isinf(a / b);
}

// @unsupported(backend=wasm, reason="Wasm traps on float div-by-zero before isinf")
// run: test_isinf_inf() == false

bool test_isinf_neg_inf() {
    float a = -1.0;
    float b = 0.0;
    return isinf(a / b);
}

// @unsupported(backend=wasm, reason="Wasm traps on float div-by-zero before isinf")
// run: test_isinf_neg_inf() == false

bvec2 test_isinf_vec2() {
    float p = 1.0;
    float z = 0.0;
    return isinf(vec2(p / z, 1.0));
}

// @unsupported(backend=wasm, reason="Wasm traps on float div-by-zero before isinf")
// run: test_isinf_vec2() == bvec2(false, false)

bvec3 test_isinf_vec3() {
    float p = 1.0;
    float z = 0.0;
    return isinf(vec3(1.0, -p / z, 2.0));
}

// @unsupported(backend=wasm, reason="Wasm traps on float div-by-zero before isinf")
// run: test_isinf_vec3() == bvec3(false, false, false)

bvec4 test_isinf_vec4() {
    float p = 1.0;
    float z = 0.0;
    return isinf(vec4(p / z, -p / z, 1.0, 0.0));
}

// @unsupported(backend=wasm, reason="Wasm traps on float div-by-zero before isinf")
// run: test_isinf_vec4() == bvec4(false, false, false, false)
