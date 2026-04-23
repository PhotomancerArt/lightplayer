// test run

// Nested callees each return `float[4]` via sret; the outer test returns the result of
// the inner call (all aggregate returns re-returned, no `T x = f();` temp assignment yet).

float[4] leaf() {
    return float[4](1.0, 2.0, 3.0, 4.0);
}

float[4] step() {
    return leaf();
}

float[4] test_call_aggregate_round_trip() {
    return step();
}

// run: test_call_aggregate_round_trip() ~= float[4](1.0, 2.0, 3.0, 4.0)
