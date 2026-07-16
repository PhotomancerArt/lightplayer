// test run

bool both_true(bool a, bool b);

bool both_true(bool a, bool b) {
    return a && b;
}

bool test_param_unnamed_bool() {
    return both_true(true, false);
}

// wgpu.f32: GPU assembly splices prototypes above the authored text; struct-typed signatures / authored prototypes break naga declaration order (tracked follow-up)
// @unsupported(wgpu.f32)
// run: test_param_unnamed_bool() == false
