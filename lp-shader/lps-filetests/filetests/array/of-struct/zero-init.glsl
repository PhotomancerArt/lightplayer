// test run

struct Point {
    float x;
    float y;
};

vec2 test_aos_zero_init() {
    Point ps[2];
    return vec2(ps[0].x, ps[0].y);
}

// run: test_aos_zero_init() ~= vec2(0.0, 0.0)
