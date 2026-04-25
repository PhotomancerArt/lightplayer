// test run

struct Point {
    float x;
    float y;
};

vec4 test_aos_dynamic_index_rw() {
    Point ps[4];
    for (int i = 0; i < 4; i++) {
        ps[i].x = float(i) * 2.0 + 1.0;
        ps[i].y = float(i) * 2.0 + 2.0;
    }
    int j = 2;
    float a = ps[j].x;
    float b = ps[j].y;
    j = 3;
    float c = ps[j].x;
    float d = ps[j].y;
    return vec4(a, b, c, d);
}

// run: test_aos_dynamic_index_rw() ~= vec4(5.0, 6.0, 7.0, 8.0)
