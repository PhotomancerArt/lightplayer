const int ITERS = 10;
const float TAU = 6.28318;

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec4 friendPattern(vec2 uv) {
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

    vec4 mapped = tanh(color * color);
    color = mapped / (1.0 + mapped);
    color.a = 1.0;
    return color;
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    return friendPattern(uv);
}