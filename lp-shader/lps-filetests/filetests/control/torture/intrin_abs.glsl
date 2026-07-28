// test run

// ============================================================================
// Control-flow torture: `abs(v)` inside loop branches
//
// A builtin call in a branch that FALLS THROUGH (no trailing return),
// with the result stored to an array, to a swizzle, or discarded.
// An emitter that leaves operands on the WASM stack fails to compile here
// even though the same call in a returning function validates.
//
// GENERATED FILE - do not edit by hand.
// Regenerate: python3 lp-shader/scripts/gen-control-torture.py --write
// ============================================================================

float test_intrin_abs_store(int count) {
    float a[8];
    for (int i = 0; i < 8; i++) {
        if (i < count) {
            float v = float(i) * 0.3125 - 1.0;
            a[i] = abs(v);
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

// run: test_intrin_abs_store(0) ~= 0.000000
// run: test_intrin_abs_store(1) ~= 1.000000
// run: test_intrin_abs_store(3) ~= 2.062500
// run: test_intrin_abs_store(8) ~= 5.000000

float test_intrin_abs_unused(int count) {
    float a[8];
    for (int i = 0; i < 8; i++) {
        a[i] = 0.0;
        if (i < count) {
            float v = float(i) * 0.3125 - 1.0;
            float dead = abs(v);
            a[i] = v + 0.0 * dead;
        }
    }
    float s = 0.0;
    for (int j = 0; j < 8; j++) {
        s = s + a[j];
    }
    return s;
}

// run: test_intrin_abs_unused(0) ~= 0.000000
// run: test_intrin_abs_unused(1) ~= -1.000000
// run: test_intrin_abs_unused(3) ~= -2.062500
// run: test_intrin_abs_unused(8) ~= 0.750000

float test_intrin_abs_swizzle(int count) {
    vec2 p[4];
    float a[4];
    for (int i = 0; i < 4; i++) {
        p[i] = vec2(0.0, 0.0);
        a[i] = 0.0;
        if (i < count) {
            float v = float(i) * 0.3125 - 1.0;
            float r = abs(v);
            vec2 q = p[i];
            q.yx = vec2(r, 0.0 - r);
            p[i] = q;
            a[i] = p[i].y + p[i].x + r;
        }
    }
    return a[0] + a[1] + a[2] + a[3];
}

// run: test_intrin_abs_swizzle(0) ~= 0.000000
// run: test_intrin_abs_swizzle(1) ~= 1.000000
// run: test_intrin_abs_swizzle(3) ~= 2.062500
// run: test_intrin_abs_swizzle(8) ~= 2.125000

float test_intrin_abs_nested(int count) {
    float a[8];
    for (int i = 0; i < 8; i++) {
        a[i] = 0.0;
        if (i < count) {
            float v = float(i) * 0.3125 - 1.0;
            if ((i / 2) * 2 == i) {
                a[i] = abs(v);
            } else {
                a[i] = 0.0 - (abs(v));
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

// run: test_intrin_abs_nested(0) ~= 0.000000
// run: test_intrin_abs_nested(1) ~= 2.000000
// run: test_intrin_abs_nested(3) ~= 3.687500
// run: test_intrin_abs_nested(8) ~= 8.000000
