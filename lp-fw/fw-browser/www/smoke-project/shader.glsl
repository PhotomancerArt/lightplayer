layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float wavePhaseA;
layout(binding = 2) uniform float wavePhaseB;
layout(binding = 3) uniform float crossPhase;
layout(binding = 4) uniform float huePhase;

const float TAU = 6.2831853;

vec3 palette(float t) {
    return 0.5 + 0.5 * cos(TAU * (t + vec3(0.0, 0.33, 0.66)));
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    float waves = sin(uv.x * 16.0 + TAU * wavePhaseA) * sin(uv.y * 14.0 - TAU * wavePhaseB);
    float cross = sin((uv.x + uv.y) * 12.0 + waves * 2.3 + TAU * crossPhase);
    float phase = uv.x * 0.55 + uv.y * 0.35 + waves * 0.12 + huePhase;
    float light = mix(0.38, 1.0, 0.5 + 0.5 * cross);

    return vec4(palette(phase) * light + vec3(0.025), 1.0);
}
