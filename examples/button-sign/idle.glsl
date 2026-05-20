layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec3 palette(float t) {
    vec3 a = vec3(0.45, 0.48, 0.52);
    vec3 b = vec3(0.40, 0.35, 0.30);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.10, 0.24, 0.38);
    return clamp(a + b * cos(6.2831853 * (c * t + d)), 0.0, 1.0);
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec2 p = (uv - 0.5) * vec2(outputSize.x / outputSize.y, 1.0);
    float n = lpfn_fbm(p * 2.8 + vec2(time * 0.035, -time * 0.025), 3, 0u);
    float radius = dot(p, p);
    float wave = 0.5 + 0.5 * sin(time * 0.35 + n * 3.2 + radius * 5.5);
    float palette_phase = mod(time * 0.04, 1.0);
    vec3 color = palette(fract(palette_phase + wave * 0.35));
    color *= mix(0.20, 0.75, smoothstep(0.1, 0.95, wave));
    return vec4(color, 1.0);
}
