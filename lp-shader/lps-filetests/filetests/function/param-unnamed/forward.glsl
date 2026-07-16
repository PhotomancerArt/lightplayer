// test run

float complex_calc(float base, int exp, bool enable);

float complex_calc(float base, int exp, bool enable) {
    if (!enable) return 0.0;
    float result = 1.0;
    for (int i = 0; i < exp; i++) {
        result = result * base;
    }
    return result;
}

float test_param_unnamed_forward_declare() {
    float result1 = complex_calc(2.0, 3, true);
    float result2 = complex_calc(2.0, 3, true);
    return result1 + result2;
}

// wgpu.f32: GPU assembly splices prototypes above the authored text; struct-typed signatures / authored prototypes break naga declaration order (tracked follow-up)
// @unsupported(wgpu.f32)
// run: test_param_unnamed_forward_declare() ~= 16.0
