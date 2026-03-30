// test run

// ============================================================================
// isnan(): Is NaN function (Q32: always false — docs/design/q32.md §6)
// ============================================================================

bool test_isnan_normal() {
    return isnan(1.0);
}

// run: test_isnan_normal() == false

bool test_isnan_zero() {
    return isnan(0.0);
}

// run: test_isnan_zero() == false

bool test_isnan_inf() {
    float a = 1.0;
    float b = 0.0;
    return isnan(a / b);
}

// @unsupported(backend=wasm, reason="Wasm traps on float div-by-zero before isnan")
// run: test_isnan_inf() == false

bool test_isnan_neg_inf() {
    float a = -1.0;
    float b = 0.0;
    return isnan(a / b);
}

// @unsupported(backend=wasm, reason="Wasm traps on float div-by-zero before isnan")
// run: test_isnan_neg_inf() == false

bvec2 test_isnan_vec2() {
    return isnan(vec2(1.0, -1.0));
}

// run: test_isnan_vec2() == bvec2(false, false)

bvec3 test_isnan_vec3() {
    return isnan(vec3(0.0, 2.0, -2.0));
}

// run: test_isnan_vec3() == bvec3(false, false, false)

bvec4 test_isnan_vec4() {
    return isnan(vec4(1.0, 0.0, -1.0, 3.0));
}

// run: test_isnan_vec4() == bvec4(false, false, false, false)
