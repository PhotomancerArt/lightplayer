// test run

// ============================================================================
// Inline-expanded ops inside blocks whose `end` is reachable.
//
// Regression class from the 2026-07-27 wasm Q32 fabs stack leak: several Q32
// ops expand inline to wasm `if`/`else`/`end` sequences (abs, min/max, the
// float<->int casts, floor/ceil/trunc, divide). A stray stack push in such an
// expansion validates fine at function top level — the trailing `return`
// makes the function's final `end` unreachable, where wasm skips the
// stack-balance check — but fails validation ("values remaining on stack at
// end of block") as soon as the op sits inside a loop or if whose block end
// is reachable. The live shader-agent sessions hit exactly that and blamed
// the surrounding control flow (break/continue in nested loops).
//
// Every function here evaluates one op family inside an if-inside-loop (the
// operand derives from the loop variable so nothing constant-folds), so a
// reintroduced leak fails compilation on the spot, on every backend.
// ============================================================================

float test_abs_in_nested_blocks() {
    float acc = 0.0;
    for (int i = 0; i < 3; i++) {
        if (i > 0) {
            acc = acc + abs(float(i) - 1.5);
        }
    }
    return acc;
}

// run: test_abs_in_nested_blocks() ~= 1.0

float test_minmax_in_nested_blocks() {
    float acc = 0.0;
    for (int i = 0; i < 3; i++) {
        if (i > 0) {
            acc = acc + min(float(i), 1.5) + max(float(i), 1.5);
        }
    }
    return acc;
}

// run: test_minmax_in_nested_blocks() ~= 6.0

float test_round_ops_in_nested_blocks() {
    float acc = 0.0;
    for (int i = 0; i < 3; i++) {
        if (i > 0) {
            float x = float(i) * 0.75;
            acc = acc + floor(x) + ceil(x) + trunc(0.0 - x);
        }
    }
    return acc;
}

// run: test_round_ops_in_nested_blocks() ~= 3.0

int test_int_cast_in_nested_blocks() {
    int acc = 0;
    for (int i = 0; i < 3; i++) {
        if (i > 0) {
            acc = acc + int(float(i) * 1.5);
        }
    }
    return acc;
}

// run: test_int_cast_in_nested_blocks() == 4

float test_uint_roundtrip_in_nested_blocks() {
    float acc = 0.0;
    for (int i = 0; i < 3; i++) {
        if (i > 0) {
            uint u = uint(float(i) * 1.5);
            acc = acc + float(u);
        }
    }
    return acc;
}

// run: test_uint_roundtrip_in_nested_blocks() ~= 4.0

float test_div_in_nested_blocks() {
    float acc = 0.0;
    for (int i = 0; i < 3; i++) {
        if (i > 0) {
            acc = acc + 3.0 / float(i);
        }
    }
    return acc;
}

// run: test_div_in_nested_blocks() ~= 4.5

// The live-session shape verbatim: abs() in the inner loop of a nested pair
// that also uses continue and break, with an array store in the same branch.
float test_abs_in_nested_loops_brkcont() {
    float acc = 0.0;
    float last[3];
    for (int i = 0; i < 3; i++) {
        last[i] = 0.0;
        for (int j = 0; j < 3; j++) {
            if (j == 2) {
                continue;
            }
            float d = abs(float(i) - float(j));
            if (d > 1.5) {
                break;
            }
            last[i] = d;
            acc = acc + d;
        }
    }
    return acc + last[1];
}

// run: test_abs_in_nested_loops_brkcont() ~= 2.0
