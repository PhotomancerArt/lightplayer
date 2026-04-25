// test run

struct Point {
    float x;
    float y;
};

vec4 test_aos_const_index_read() {
    Point ps[2];
    ps[0].x = 1.0;
    ps[0].y = 2.0;
    ps[1].x = 3.0;
    ps[1].y = 4.0;
    return vec4(ps[0].x, ps[0].y, ps[1].x, ps[1].y);
}

// run: test_aos_const_index_read() ~= vec4(1.0, 2.0, 3.0, 4.0)

float test_aos_dynamic_index_sum() {
    Point ps[2];
    ps[0].x = 10.0;
    ps[1].x = 20.0;
    float s = 0.0;
    for (int i = 0; i < 2; i++) {
        s += ps[i].x;
    }
    return s;
}

// run: test_aos_dynamic_index_sum() == 30.0

float test_aos_member_write_dynamic() {
    Point ps[2];
    for (int i = 0; i < 2; i++) {
        ps[i].x = float(i) + 1.0;
    }
    return ps[0].x + ps[1].x;
}

// run: test_aos_member_write_dynamic() == 3.0
