// test run

struct Point {
    float x;
    float y;
};

void fill_aos(out Point ps[2]) {
    ps[0] = Point(30.0, 40.0);
}

vec2 test_aos_out_param() {
    Point ps[2];
    fill_aos(ps);
    return vec2(ps[0].x, ps[0].y);
}

// wgpu.f32: GPU assembly splices prototypes above the authored text; struct-typed signatures / authored prototypes break naga declaration order (tracked follow-up)
// @unsupported(wgpu.f32)
// run: test_aos_out_param() ~= vec2(30.0, 40.0)
