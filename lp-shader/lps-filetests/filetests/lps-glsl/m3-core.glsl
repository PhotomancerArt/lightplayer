// test run

float choose(float x) {
    if (x < 0.5) {
        return 1.0;
    } else {
        return 2.0;
    }
}

vec4 test_core(vec2 pos) {
    vec2 flipped = pos.yx;
    float wave = sin(0.0) + cos(0.0);
    float ramp = smoothstep(0.0, 1.0, 0.5);
    vec3 color = clamp(vec3(flipped, ramp) * wave, 0.0, 1.0);
    return vec4(color, choose(pos.x));
}

// run: choose(0.25) ~= 1.0
// run: choose(0.75) ~= 2.0
// run: test_core(vec2(0.25, 0.75)) ~= vec4(0.75, 0.25, 0.5, 1.0) (tolerance: 0.002)
