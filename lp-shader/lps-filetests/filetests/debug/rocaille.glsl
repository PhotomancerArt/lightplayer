// test run
//
// Debug reproduction for examples/rocaille/src/rainbow.shader/main.glsl.
// These tests intentionally assert only that the shader body completes; a trap
// before the comparison is the failure mode being investigated.

const int ITERS = 10;
const float TAU = 6.28318;

vec4 rocailleAccum(vec2 uv, float time) {
    vec2 v = vec2(1.0, 1.0);
    vec2 p = (uv + uv - v) / 0.3;
    vec4 color = vec4(0.0);
    float phase = mod(time * 0.05 * TAU, TAU);

    for (int i = 1; i < ITERS; i++) {
        v = p;
        for (int f = 1; f < ITERS; f++) {
            float ff = float(f);
            v += sin(v.yx * ff + float(i) + phase) / ff;
        }

        vec4 ramp = cos(float(i) + vec4(0.0, 1.0, 2.0, 3.0)) + 1.0;
        color += ramp / 6.0 / max(length(v), 0.001);
    }

    color.a = 1.0;
    return color;
}

vec4 rocailleNoTanh(vec2 pos, vec2 outputSize, float time) {
    vec4 color = rocailleAccum(pos / outputSize, time);
    color = color / (1.0 + color);
    color.a = 1.0;
    return color;
}

vec4 rocailleWithTanh(vec2 pos, vec2 outputSize, float time) {
    vec4 color = rocailleAccum(pos / outputSize, time);
    vec4 mapped = tanh(color * color);
    color = mapped / (1.0 + mapped);
    color.a = 1.0;
    return color;
}

float consumeColor(vec4 color) {
    return color.r + color.g * 0.5 + color.b * 0.25 + color.a * 0.125;
}

bool test_rocaille_accum_center_t0() {
    float v = consumeColor(rocailleAccum(vec2(0.5, 0.5), 0.0));
    return v > -1.0;
}

// run: test_rocaille_accum_center_t0() == true

bool test_rocaille_no_tanh_center_t25() {
    float v = consumeColor(rocailleNoTanh(vec2(8.5, 8.5), vec2(16.0, 16.0), 2.5));
    return v > -1.0;
}

// run: test_rocaille_no_tanh_center_t25() == true

bool test_rocaille_no_tanh_corner_t25() {
    float v = consumeColor(rocailleNoTanh(vec2(0.5, 0.5), vec2(16.0, 16.0), 2.5));
    return v > -1.0;
}

// run: test_rocaille_no_tanh_corner_t25() == true

bool test_rocaille_with_tanh_center_t25() {
    float v = consumeColor(rocailleWithTanh(vec2(8.5, 8.5), vec2(16.0, 16.0), 2.5));
    return v > -1.0;
}

// run: test_rocaille_with_tanh_center_t25() == true

bool test_rocaille_no_tanh_row_0_t25() {
    float total = 0.0;
    for (int x = 0; x < 16; x++) {
        total += consumeColor(rocailleNoTanh(vec2(float(x) + 0.5, 0.5), vec2(16.0, 16.0), 2.5));
    }
    return total > -1.0;
}

// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_0_t25() == true

bool test_rocaille_no_tanh_row_t25(int y) {
    float total = 0.0;
    for (int x = 0; x < 16; x++) {
        total += consumeColor(rocailleNoTanh(vec2(float(x) + 0.5, float(y) + 0.5), vec2(16.0, 16.0), 2.5));
    }
    return total > -1.0;
}

// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(1) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(2) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(3) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(4) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(5) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(6) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(7) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(8) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(9) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(10) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(11) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(12) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(13) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(14) == true
// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_no_tanh_row_t25(15) == true

bool test_rocaille_with_tanh_row_0_t25() {
    float total = 0.0;
    for (int x = 0; x < 16; x++) {
        total += consumeColor(rocailleWithTanh(vec2(float(x) + 0.5, 0.5), vec2(16.0, 16.0), 2.5));
    }
    return total > -1.0;
}

// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_with_tanh_row_0_t25() == true
