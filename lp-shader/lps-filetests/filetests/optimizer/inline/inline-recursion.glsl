// test run

// ============================================================================
// Inliner: deep call chain (no cycles). GLSL forbids recursion; a mistaken
// "recursive" inline would miscompile or panic — this chain stresses that.
// ============================================================================

int chain9(int x) {
    return x;
}

int chain8(int x) {
    return chain9(x + 1);
}

int chain7(int x) {
    return chain8(x + 1);
}

int chain6(int x) {
    return chain7(x + 1);
}

int chain5(int x) {
    return chain6(x + 1);
}

int chain4(int x) {
    return chain5(x + 1);
}

int chain3(int x) {
    return chain4(x + 1);
}

int chain2(int x) {
    return chain3(x + 1);
}

int chain1(int x) {
    return chain2(x + 1);
}

int chain0(int x) {
    return chain1(x + 1);
}

int test_inline_deep_chain() {
    return chain0(0);
}

// run: test_inline_deep_chain() == 9

float tail(float x) {
    return x * 2.0;
}

float step4(float x) {
    return tail(x + 1.0);
}

float step3(float x) {
    return step4(x) + 1.0;
}

float step2(float x) {
    return step3(x * 2.0);
}

float step1(float x) {
    return step2(x + 0.5);
}

float test_inline_deep_chain_float() {
    return step1(1.0);
}

// step1(1)=step2(1.5)=step3(3.0)=step4(3.0)+1=tail(4.0)+1=8+1=9
// run: test_inline_deep_chain_float() ~= 9.0
