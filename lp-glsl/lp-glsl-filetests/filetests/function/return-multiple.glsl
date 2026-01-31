// test run
// target riscv32.q32

// ============================================================================
// Multiple Return Paths: Functions with multiple return statements
// ============================================================================

float classify_number(float x) {
    if (x > 0.0) {
        return 1.0; // Positive
    } else if (x < 0.0) {
        return -1.0; // Negative
    } else {
        return 0.0; // Zero
    }
}

float test_return_multiple_conditional() {
    // Multiple returns based on conditions
    return classify_number(5.0) + classify_number(-3.0) + classify_number(0.0);
}

// run: test_return_multiple_conditional() ~= 0.0 [expect-fail]

int categorize_value(int x) {
    if (x < 10) {
        return 0; // Low
    } else if (x < 50) {
        return 1; // Medium
    } else if (x < 100) {
        return 2; // High
    } else {
        return 3; // Very high
    }
}

int test_return_multiple_ranges() {
    // Multiple returns for different ranges
    return categorize_value(5) + categorize_value(25) + categorize_value(75) + categorize_value(150);
}

// run: test_return_multiple_ranges() == 6 [expect-fail]

float get_multiplier(int category) {
    if (category == 0) {
        return 1.0;
    } else if (category == 1) {
        return 2.0;
    } else if (category == 2) {
        return 3.0;
    } else {
        return 0.5;
    }
}

float test_return_multiple_switch_like() {
    // Multiple returns simulating switch behavior
    return get_multiplier(0) + get_multiplier(1) + get_multiplier(2) + get_multiplier(3);
}

// run: test_return_multiple_switch_like() ~= 6.5 [expect-fail]

vec2 get_direction(int dir) {
    if (dir == 0) {
        return vec2(1.0, 0.0); // Right
    } else if (dir == 1) {
        return vec2(0.0, 1.0); // Up
    } else if (dir == 2) {
        return vec2(-1.0, 0.0); // Left
    } else {
        return vec2(0.0, -1.0); // Down
    }
}

vec2 test_return_multiple_vector() {
    // Multiple returns with vectors
    vec2 dir1 = get_direction(0);
    vec2 dir2 = get_direction(1);
    return dir1 + dir2;
}

// run: test_return_multiple_vector() ~= vec2(1.0, 1.0) [expect-fail]

bool validate_input(float x, float min_val, float max_val) {
    if (x < min_val) {
        return false; // Too small
    } else if (x > max_val) {
        return false; // Too large
    } else {
        return true; // Valid
    }
}

bool test_return_multiple_bool() {
    // Multiple boolean returns
    return validate_input(5.0, 0.0, 10.0) && validate_input(15.0, 0.0, 10.0);
}

// run: test_return_multiple_bool() == false [expect-fail]

float complex_logic(int a, int b, bool flag) {
    if (a > b) {
        if (flag) {
            return float(a - b);
        } else {
            return float(a + b);
        }
    } else {
        if (flag) {
            return float(b - a);
        } else {
            return float(a * b);
        }
    }
}

float test_return_multiple_nested() {
    // Multiple returns in nested conditions
    return complex_logic(5, 3, true) + complex_logic(3, 5, false);
}

// run: test_return_multiple_nested() ~= 11.0 [expect-fail]

int find_special_value(int[5] arr) {
    for (int i = 0; i < 5; i++) {
        if (arr[i] == 42) {
            return i; // Found at index i
        } else if (arr[i] < 0) {
            return -1; // Invalid negative value
        }
    }
    return -2; // Not found, all valid
}

int test_return_multiple_loop() {
    // Multiple returns inside loops
    int[5] data1 = int[5](1, 2, 42, 4, 5);
    int[5] data2 = int[5](1, -3, 42, 4, 5);
    return find_special_value(data1) + find_special_value(data2) + 10; // 2 + (-1) + 10 = 11
}

// run: test_return_multiple_loop() == 11 [expect-fail]

float process_sequence(float[4] arr) {
    if (arr[0] < 0.0) {
        return -1.0; // Early exit for invalid
    }
    if (arr[1] < 0.0) {
        return -2.0; // Different early exit
    }
    if (arr[2] < 0.0) {
        return -3.0; // Another early exit
    }
    // Process normally
    return arr[0] + arr[1] + arr[2] + arr[3];
}

float test_return_multiple_early_exit() {
    // Mix of early returns and final return
    float[4] data = float[4](1.0, 2.0, 3.0, 4.0);
    return process_sequence(data);
}

// run: test_return_multiple_early_exit() ~= 10.0 [expect-fail]

void process_with_exits(int x) {
    if (x < 0) {
        return; // Early exit 1
    }
    if (x == 0) {
        return; // Early exit 2
    }
    if (x > 100) {
        return; // Early exit 3
    }
    // Continue processing
}

void test_return_multiple_void() {
    // Multiple returns in void function
    process_with_exits(50);
}

// run: test_return_multiple_void() == 0.0 [expect-fail]
