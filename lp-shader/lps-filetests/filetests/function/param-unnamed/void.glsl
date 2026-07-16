// test run

void process(float value, int count);

void process(float value, int count) {}

void test_param_unnamed_void() {
    process(5.0, 3);
}

// wgpu.f32: GPU assembly splices prototypes above the authored text; struct-typed signatures / authored prototypes break naga declaration order (tracked follow-up)
// @unsupported(wgpu.f32)
// run: test_param_unnamed_void() == 0.0
