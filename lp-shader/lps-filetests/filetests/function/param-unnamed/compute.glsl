// test run

float compute(float a, float b, float c);

float compute(float a, float b, float c) {
    return a * b + c;
}

float test_param_unnamed_all_unnamed() {
    return compute(2.0, 3.0, 4.0);
}

// wgpu.f32: GPU assembly splices prototypes above the authored text; struct-typed signatures / authored prototypes break naga declaration order (tracked follow-up)
// @unsupported(wgpu.f32)
// run: test_param_unnamed_all_unnamed() ~= 10.0
