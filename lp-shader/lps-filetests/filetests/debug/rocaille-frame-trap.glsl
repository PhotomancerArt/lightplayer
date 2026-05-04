// test run
//
// Isolated reproduction for the rocaille full-frame WASM trap.

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

bool test_rocaille_with_tanh_frame_t25() {
    for (int y = 0; y < 42; y++) {
        rocailleAccum(vec2(0,0), 2.5);
    }
    return true;
}

// @unsupported(rv32n.q32)
// @unsupported(rv32c.q32)
// run: test_rocaille_with_tanh_frame_t25() == true
