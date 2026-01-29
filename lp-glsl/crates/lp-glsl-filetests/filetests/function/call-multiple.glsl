// test run
// target riscv32.q32

// ============================================================================
// Multiple Function Calls: Same function called multiple times
// ============================================================================

float square_multiple(float x) {
    return x * x;
}

float test_call_multiple_same_args() {
    // Call same function multiple times with same arguments
    return square_multiple(3.0) + square_multiple(3.0) + square_multiple(3.0); // 9 + 9 + 9 = 27
}

// run: test_call_multiple_same_args() ~= 27.0

float power_multiple(float base, float exp) {
    // Manual power calculation instead of pow()
    if (exp == 1.0) return base;
    if (exp == 2.0) return base * base;
    if (exp == 3.0) return base * base * base;
    return base; // fallback
}

float test_call_multiple_different_args() {
    // Call same function multiple times with different arguments
    return power_multiple(2.0, 1.0) + power_multiple(2.0, 2.0) + power_multiple(2.0, 3.0); // 2 + 4 + 8 = 14
}

// run: test_call_multiple_different_args() ~= 14.0

vec2 rotate90_multiple(vec2 v) {
    return vec2(-v.y, v.x);
}

vec2 test_call_multiple_vector() {
    // Multiple calls with vector functions
    vec2 v1 = rotate90_multiple(vec2(1.0, 0.0));  // (0, 1)
    vec2 v2 = rotate90_multiple(vec2(0.0, 1.0));  // (-1, 0)
    vec2 v3 = rotate90_multiple(vec2(-1.0, 0.0)); // (0, -1)
    return v1 + v2 + v3; // (0+(-1)+0, 1+0+(-1)) = (-1, 0)
}

// run: test_call_multiple_vector() ~= vec2(-1.0, 0.0)

float fibonacci_multiple(int n) {
    if (n <= 1) return float(n);
    float a = 0.0, b = 1.0, temp;
    for (int i = 2; i <= n; i++) {
        temp = a + b;
        a = b;
        b = temp;
    }
    return b;
}

float test_call_multiple_accumulate() {
    // Accumulate results from multiple calls
    float sum = 0.0;
    for (int i = 0; i < 5; i++) {
        sum = sum + fibonacci_multiple(i);
    }
    return sum; // fib(0)=0, fib(1)=1, fib(2)=1, fib(3)=2, fib(4)=3, sum=7
}

// run: test_call_multiple_accumulate() ~= 7.0

float scale_by_index_multiple(float base, int index) {
    return base * float(index + 1);
}

float test_call_multiple_in_loop() {
    // Function calls inside loops
    float total = 0.0;
    for (int i = 0; i < 3; i++) {
        total = total + scale_by_index_multiple(2.0, i);
    }
    return total; // 2*1 + 2*2 + 2*3 = 2 + 4 + 6 = 12
}

// run: test_call_multiple_in_loop() ~= 12.0

void log_value_multiple(float x) {
    // In real shader would log, here just call
}

void test_call_multiple_void() {
    // Multiple calls to void functions
    log_value_multiple(1.0);
    log_value_multiple(2.0);
    log_value_multiple(3.0);
}

// run: test_call_multiple_void() == 0.0

float add_multiple(float a, float b) {
    return a + b;
}

float multiply_multiple(float a, float b) {
    return a * b;
}

float test_call_multiple_nested() {
    // Multiple calls in nested expressions
    // ((1+2) * (3+4)) + ((5+6) * (7+8))
    return multiply_multiple(add_multiple(1.0, 2.0), add_multiple(3.0, 4.0)) +
           multiply_multiple(add_multiple(5.0, 6.0), add_multiple(7.0, 8.0));
}

// run: test_call_multiple_nested() ~= 186.0
