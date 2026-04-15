// test run

void set_components(out ivec3 result) {
    result.x = 1;
    result.y = 2;
    result.z = 3;
}

float test_edge_out_modify_components() {
    // Can modify individual components without reading whole vector
    ivec3 v;
    set_components(v);
    return v.x + v.y + v.z;
}

// run: test_edge_out_modify_components() == 6
