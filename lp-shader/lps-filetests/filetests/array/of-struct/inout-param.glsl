// test run

struct Point {
    float x;
    float y;
};

void modify_aos(inout Point ps[2]) {
    ps[0].x = 10.0;
    ps[0].y = 20.0;
}

vec2 test_aos_inout_param() {
    Point ps[2];
    ps[0].x = 0.0;
    ps[0].y = 0.0;
    modify_aos(ps);
    return vec2(ps[0].x, ps[0].y);
}

// run: test_aos_inout_param() ~= vec2(10.0, 20.0)
