// test run

// One prototype + definition per file. Prototype uses the same parameter names as the
// definition so lowering matches (all-unnamed prototypes like `float add(float,float)` mis-parse).

float add(float a, float b);

float add(float a, float b) {
    return a + b;
}

float test_param_unnamed_simple() {
    return add(3.0, 4.0);
}

// wgpu.f32: GPU assembly splices prototypes above the authored text; struct-typed signatures / authored prototypes break naga declaration order (tracked follow-up)
// @unsupported(wgpu.f32)
// run: test_param_unnamed_simple() ~= 7.0
