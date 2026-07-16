// test run

float multiply(float, int count);

float multiply(float factor, int count) {
    float result = 1.0;
    for (int i = 0; i < count; i++) {
        result = result * factor;
    }
    return result;
}

float test_param_unnamed_mixed() {
    return multiply(2.0, 3);
}

// wgpu.f32: GPU assembly splices prototypes above the authored text; struct-typed signatures / authored prototypes break naga declaration order (tracked follow-up)
// @unsupported(wgpu.f32)
// run: test_param_unnamed_mixed() ~= 8.0
