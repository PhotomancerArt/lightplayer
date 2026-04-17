// test run

// ============================================================================
// Inliner: many small helpers with interleaved call graph (topo stress).
// ============================================================================

float m1(float x) {
    return x + 1.0;
}

float m2(float x) {
    return m1(x) * 2.0;
}

float m3(float x) {
    return m2(x) - m1(0.0);
}

float m4(float x) {
    return m3(x) + m2(0.5);
}

float m5(float x) {
    return m4(x) * m1(0.0);
}

float m6(float x) {
    return m5(x) + m3(0.0);
}

float m7(float x) {
    return m6(x) + m4(0.0);
}

float m8(float x) {
    return m7(m2(x));
}

float m9(float x) {
    return m8(x) - m5(0.0);
}

float m10(float x) {
    return m9(x) + m6(0.0);
}

float test_inline_many_small() {
    return m10(1.0);
}

// x=1: m10=18 (traced from m1..m9 definitions).
// run: test_inline_many_small() ~= 18.0
