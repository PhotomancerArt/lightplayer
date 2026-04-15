// test run

// ============================================================================
// Array Return Types: Arrays must be explicitly sized
// ============================================================================

vec3 generate_sequence(float start, float step) {
    return vec3(start, 1.0 + step, start + step);
}

// run: generate_sequence(1.0, 0.5) ~= vec3(1.0, 1.5, 1.5)
