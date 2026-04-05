// test run

// Chained scalar `&&` / `||` with float comparisons (Q32 WASM regression: left bool must not share
// temps with the right-hand lowering).

float test_chain_and_two_float_cmps() {
    float a = 1.0;
    float b = 0.5;
    return (a < 2.0 && b < 1.0) ? 1.0 : 0.0;
}

// run: test_chain_and_two_float_cmps() ~= 1.0

float test_chain_or_float_cmps() {
    float a = 3.0;
    float b = 0.5;
    return (a < 2.0 || b < 1.0) ? 1.0 : 0.0;
}

// run: test_chain_or_float_cmps() ~= 1.0

float test_chain_and_three_float_cmps() {
    float a = 1.0;
    float b = 0.5;
    float c = 0.25;
    return (a < 2.0 && b < 1.0 && c < 0.5) ? 1.0 : 0.0;
}

// run: test_chain_and_three_float_cmps() ~= 1.0

float test_bool_local_from_float_chain() {
    float a = 1.0;
    float b = 0.5;
    bool ok = a < 2.0 && b < 1.0;
    return ok ? 1.0 : 0.0;
}

// run: test_bool_local_from_float_chain() ~= 1.0
