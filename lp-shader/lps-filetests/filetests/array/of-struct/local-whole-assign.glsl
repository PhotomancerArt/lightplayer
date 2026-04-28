// test run

struct Point {
    float x;
    float y;
};

vec2 test_aos_whole_assign_from_local() {
    Point ps[2];
    Point q = Point(5.0, 6.0);
    ps[0] = q;
    return vec2(ps[0].x, ps[0].y);
}

// run: test_aos_whole_assign_from_local() ~= vec2(5.0, 6.0)
