// test run
// target riscv32.q32

// ============================================================================
// Early Return Statements: Return before end of function
// ============================================================================

float absolute_value(float x) {
    if (x >= 0.0) {
        return x;
    }
    return -x;
}

float test_return_early_simple() {
    // Early return in simple case
    return absolute_value(-5.0);
}

// run: test_return_early_simple() ~= 5.0 [expect-fail]

int find_first_positive(int[5] arr) {
    for (int i = 0; i < 5; i++) {
        if (arr[i] > 0) {
            return arr[i];
        }
    }
    return -1; // Not found
}

int test_return_early_loop() {
    // Early return inside loop
    int[5] data = int[5](-1, -2, 3, -4, 5);
    return find_first_positive(data);
}

// run: test_return_early_loop() == 3 [expect-fail]

float process_value(float x) {
    if (x > 10.0) {
        if (x > 20.0) {
            return x * 2.0;
        }
        return x + 5.0;
    }
    return x;
}

float test_return_early_nested() {
    // Early return in nested conditions
    return process_value(25.0);
}

// run: test_return_early_nested() ~= 50.0 [expect-fail]

bool contains_negative(float[3] arr) {
    if (arr[0] < 0.0) return true;
    if (arr[1] < 0.0) return true;
    if (arr[2] < 0.0) return true;
    return false;
}

bool test_return_early_bool() {
    // Early return with boolean logic
    float[3] values = float[3](1.0, -2.0, 3.0);
    return contains_negative(values);
}

// run: test_return_early_bool() == true [expect-fail]

float safe_divide(float a, float b) {
    if (b == 0.0) {
        return 0.0; // Avoid division by zero
    }
    return a / b;
}

float test_return_early_math() {
    // Early return in mathematical function
    return safe_divide(10.0, 0.0);
}

// run: test_return_early_math() ~= 0.0 [expect-fail]

vec2 clamp_vector(vec2 v, float max_len) {
    float len = length(v);
    if (len <= max_len) {
        return v;
    }
    return normalize(v) * max_len;
}

vec2 test_return_early_vector() {
    // Early return with vectors
    vec2 long_vector = vec2(10.0, 0.0);
    return clamp_vector(long_vector, 5.0);
}

// run: test_return_early_vector() ~= vec2(5.0, 0.0) [expect-fail]

int index_of(int[4] arr, int target) {
    if (arr[0] == target) return 0;
    if (arr[1] == target) return 1;
    if (arr[2] == target) return 2;
    if (arr[3] == target) return 3;
    return -1; // Not found
}

int test_return_early_search() {
    // Early return in search function
    int[4] data = int[4](10, 20, 30, 40);
    return index_of(data, 30);
}

// run: test_return_early_search() == 2 [expect-fail]

float complex_calculation(float x, float y, bool use_addition) {
    if (x < 0.0) {
        return 0.0; // Early exit for negative x
    }

    if (use_addition) {
        if (x + y > 100.0) {
            return 100.0; // Cap result
        }
        return x + y;
    } else {
        if (x * y < 1.0) {
            return 1.0; // Minimum result
        }
        return x * y;
    }
}

float test_return_early_complex() {
    // Complex early return logic
    return complex_calculation(10.0, 15.0, true);
}

// run: test_return_early_complex() ~= 25.0 [expect-fail]

void process_until_negative(float[4] arr) {
    // Process elements until we find a negative
    if (arr[0] >= 0.0) {
        // process arr[0]
        if (arr[1] >= 0.0) {
            // process arr[1]
            if (arr[2] >= 0.0) {
                // process arr[2]
                return; // Early exit
            }
        }
    }
}

void test_return_early_void() {
    // Early return in void function
    float[4] data = float[4](1.0, 2.0, -3.0, 4.0);
    process_until_negative(data);
}

// run: test_return_early_void() == 0.0 [expect-fail]
