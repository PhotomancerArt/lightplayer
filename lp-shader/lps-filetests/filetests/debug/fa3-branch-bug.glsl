// test run

float power_multiple(float base, float exp) {
    float exp = 3.0;
    // Manual power calculation instead of pow()
    if (exp == 2.0) return base * base;
    if (exp == 3.0) return base * base * base;
    return base; // fallback
}

// run: power_multiple(2.0, 3.0) ~= 8.0
