// test run

// ============================================================================
// Control-flow torture: `sqrt(v)` inside loop branches
//
// A builtin call in a branch that FALLS THROUGH (no trailing return),
// with the result stored to an array, to a swizzle, or discarded.
// An emitter that leaves operands on the WASM stack fails to compile here
// even though the same call in a returning function validates.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

float test_intrin_sqrt_store(int count) {
    float a[8];
    for (int i = 0; i < 8; i++) {
        if (i < count) {
            float v = float(i) * 0.5 + 0.25;
            a[i] = sqrt(v);
        } else {
            a[i] = 0.0;
        }
    }
    float s = 0.0;
    for (int j = 0; j < 8; j++) {
        s = s + a[j];
    }
    return s;
}

// run: test_intrin_sqrt_store(0) ~= 0.000000
// run: test_intrin_sqrt_store(1) ~= 0.500000
// run: test_intrin_sqrt_store(3) ~= 2.484059
// run: test_intrin_sqrt_store(8) ~= 10.704515

float test_intrin_sqrt_unused(int count) {
    float a[8];
    for (int i = 0; i < 8; i++) {
        a[i] = 0.0;
        if (i < count) {
            float v = float(i) * 0.5 + 0.25;
            float dead = sqrt(v);
            a[i] = v + 0.0 * dead;
        }
    }
    float s = 0.0;
    for (int j = 0; j < 8; j++) {
        s = s + a[j];
    }
    return s;
}

// run: test_intrin_sqrt_unused(0) ~= 0.000000
// run: test_intrin_sqrt_unused(1) ~= 0.250000
// run: test_intrin_sqrt_unused(3) ~= 2.250000
// run: test_intrin_sqrt_unused(8) ~= 16.000000

float test_intrin_sqrt_swizzle(int count) {
    vec2 p[4];
    float a[4];
    for (int i = 0; i < 4; i++) {
        p[i] = vec2(0.0, 0.0);
        a[i] = 0.0;
        if (i < count) {
            float v = float(i) * 0.5 + 0.25;
            float r = sqrt(v);
            vec2 q = p[i];
            q.yx = vec2(r, 0.0 - r);
            p[i] = q;
            a[i] = p[i].y + p[i].x + r;
        }
    }
    return a[0] + a[1] + a[2] + a[3];
}

// run: test_intrin_sqrt_swizzle(0) ~= 0.000000
// run: test_intrin_sqrt_swizzle(1) ~= 0.500000
// run: test_intrin_sqrt_swizzle(3) ~= 2.484059
// run: test_intrin_sqrt_swizzle(8) ~= 3.806935

float test_intrin_sqrt_nested(int count) {
    float a[8];
    for (int i = 0; i < 8; i++) {
        a[i] = 0.0;
        if (i < count) {
            float v = float(i) * 0.5 + 0.25;
            if ((i / 2) * 2 == i) {
                a[i] = sqrt(v);
            } else {
                a[i] = 0.0 - (sqrt(v));
            }
            a[i] = a[i] + 1.0;
        }
    }
    float s = 0.0;
    for (int j = 0; j < 8; j++) {
        s = s + a[j];
    }
    return s;
}

// run: test_intrin_sqrt_nested(0) ~= 0.000000
// run: test_intrin_sqrt_nested(1) ~= 1.500000
// run: test_intrin_sqrt_nested(3) ~= 3.752009
// run: test_intrin_sqrt_nested(8) ~= 7.137104
