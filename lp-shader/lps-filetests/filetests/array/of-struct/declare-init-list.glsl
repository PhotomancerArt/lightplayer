// test run

struct Point {
    float x;
    float y;
};

vec4 test_aos_declare_init_list() {
    Point ps[2] = Point[2](Point(1.0, 2.0), Point(3.0, 4.0));
    return vec4(ps[0].x, ps[0].y, ps[1].x, ps[1].y);
}

// run: test_aos_declare_init_list() ~= vec4(1.0, 2.0, 3.0, 4.0)
