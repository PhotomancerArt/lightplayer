// test run

void set_components(out vec3 result) {
    result.x = 1.0;
    result.y = 2.0;
    result.z = 3.0;
}

float test_edge_out_modify_components() {
    // Can modify individual components without reading whole vector
    vec3 v;
    set_components(v);
    return v.x + v.y + v.z;
}

// run: test_edge_out_modify_components() ~= 6.0
