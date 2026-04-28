// test run

struct Point {
    float x;
    float y;
};

struct Container {
    Point ps[2];
};

vec2 test_aos_nested_field_rw() {
    Container c;
    c.ps[0].x = 100.0;
    c.ps[0].y = 200.0;
    return vec2(c.ps[0].x, c.ps[0].y);
}

// run: test_aos_nested_field_rw() ~= vec2(100.0, 200.0)
