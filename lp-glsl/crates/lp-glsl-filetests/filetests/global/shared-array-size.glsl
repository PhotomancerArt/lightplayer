// test run
// target riscv32.q32

// ============================================================================
// Shared Array Size: Shared global arrays must have same size across shaders
// ============================================================================

// Shared uniform arrays - sizes must match across shaders
uniform vec4 shared_colors[4];
uniform mat3 shared_transforms[2];
uniform float shared_weights[8];
uniform int shared_indices[16];

// These arrays would need to have the same sizes in all shaders
// that declare them

vec4 test_shared_array_size_colors() {
    // Access shared color array
    vec4 sum = vec4(0.0);
    for (int i = 0; i < 4; i++) {
        sum = sum + shared_colors[i];
    }
    return sum;
}

// run: test_shared_array_size_colors() ~= vec4(0.0, 0.0, 0.0, 0.0) [expect-fail]

mat3 test_shared_array_size_transforms() {
    // Access shared transform array
    mat3 combined = mat3(0.0);
    for (int i = 0; i < 2; i++) {
        combined = combined + shared_transforms[i];
    }
    return combined;
}

// run: test_shared_array_size_transforms() ~= mat3(0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0) [expect-fail]

float test_shared_array_size_weights() {
    // Access shared weights array
    float total = 0.0;
    for (int i = 0; i < 8; i++) {
        total = total + shared_weights[i];
    }
    return total;
}

// run: test_shared_array_size_weights() ~= 0.0 [expect-fail]

int test_shared_array_size_indices() {
    // Access shared indices array
    int max_index = 0;
    for (int i = 0; i < 16; i++) {
        if (shared_indices[i] > max_index) {
            max_index = shared_indices[i];
        }
    }
    return max_index;
}

// run: test_shared_array_size_indices() == 0 [expect-fail]

float test_shared_array_size_combined() {
    // Combined access to shared arrays
    float result = 0.0;

    // Sum first elements
    result = result + shared_colors[0].x;
    result = result + shared_transforms[0][0][0];
    result = result + shared_weights[0];
    result = result + float(shared_indices[0]);

    return result;
}

// run: test_shared_array_size_combined() ~= 0.0 [expect-fail]
