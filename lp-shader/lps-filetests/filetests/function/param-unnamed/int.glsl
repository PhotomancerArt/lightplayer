// test run

int max_value(int a, int b);

int max_value(int a, int b) {
    return a > b ? a : b;
}

int test_param_unnamed_int() {
    return max_value(5, 8);
}

// wgpu.f32: GPU assembly splices prototypes above the authored text; struct-typed signatures / authored prototypes break naga declaration order (tracked follow-up)
// @unsupported(wgpu.f32)
// run: test_param_unnamed_int() == 8
