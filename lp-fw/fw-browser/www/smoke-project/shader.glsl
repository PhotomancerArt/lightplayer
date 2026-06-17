layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

const float TAU = 6.28318;

vec3 palette(float t) {
    return 0.5 + 0.5 * cos(TAU * (t + vec3(0.0, 0.33, 0.66)));
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    float waves = sin(uv.x * 16.0 + time * 2.1) * sin(uv.y * 14.0 - time * 1.7);
    float cross = sin((uv.x + uv.y) * 12.0 + waves * 2.3 + time * 1.3);
    float phase = uv.x * 0.55 + uv.y * 0.35 + waves * 0.12 + time * 0.08;
    float light = mix(0.38, 1.0, 0.5 + 0.5 * cross);

    return vec4(palette(phase) * light + vec3(0.025), 1.0);
}
