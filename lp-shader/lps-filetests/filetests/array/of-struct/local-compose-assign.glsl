// test run

struct Point {
    float x;
    float y;
};

vec2 test_aos_whole_assign_compose() {
    Point ps[2];
    ps[0] = Point(7.0, 8.0);
    return vec2(ps[0].x, ps[0].y);
}

// run: test_aos_whole_assign_compose() ~= vec2(7.0, 8.0)
