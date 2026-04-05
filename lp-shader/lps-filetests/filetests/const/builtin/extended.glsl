// test run

// Spec: variables.adoc §4.3.3.1 "Constant Expressions"
// Extended builtins: trunc, round, ceil, mod, exp, log, exp2, log2, degrees, asin, acos.

const float T = trunc(2.7);
// `round(2.5)` in a const expression is const-folded by Naga with ties-to-even; use `round(3.5)`
// so the folded value is 4 on both Naga and GLSL half-away-from-zero (see `builtins/common-round.glsl` for runtime `round(2.5)`).
const float R = round(3.5);
const float C = ceil(2.1);
const float M = mod(5.0, 2.0);

float test_builtin_trunc_round_ceil_mod() {
    return T + R + C + M;
}

// run: test_builtin_trunc_round_ceil_mod() ~= 10.0

const float E = exp(0.0);
const float L = log(1.0);
const float E2 = exp2(3.0);
const float L2 = log2(8.0);

float test_builtin_exp_log() {
    return E + L + E2 + L2;
}

// run: test_builtin_exp_log() ~= 12.0

const float RAD90 = 1.570796;
const float DEG = degrees(RAD90);
const float AS = asin(1.0);
const float AC = acos(0.0);

float test_builtin_degrees_asin_acos() {
    return DEG + AS + AC;
}

// run: test_builtin_degrees_asin_acos() ~= 93.1416
