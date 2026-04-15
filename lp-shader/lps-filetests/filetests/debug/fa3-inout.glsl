// test run

void adjust_components(inout vec3 v) {
    v.x = v.x + 1.0;
}

float test_param_inout_modify_components() {
    // Modify components of inout vector
    vec3 vec = vec3(1.0, 2.0, 3.0);
    adjust_components(vec);
    return vec.x + vec.y + vec.z;
}

// run: test_param_inout_modify_components() ~= 7.0
