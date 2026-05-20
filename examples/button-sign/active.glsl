layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;

vec3 neon(float t) {
    vec3 a = vec3(0.55, 0.45, 0.55);
    vec3 b = vec3(0.55, 0.55, 0.45);
    vec3 c = vec3(1.0, 1.0, 1.0);
    vec3 d = vec3(0.00, 0.33, 0.67);
    return clamp(a + b * cos(6.2831853 * (c * t + d)), 0.0, 1.0);
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec2 p = uv - 0.5;
    float aspect = outputSize.x / outputSize.y;
    p.x *= aspect;

    float radius = dot(p, p);
    float a = atan(p.y, p.x);
    float spokes = sin(a * 12.0 + time * 13.0);
    float rings = sin(radius * 58.0 - time * 18.0);
    float blast = smoothstep(0.75, 1.0, spokes * 0.55 + rings * 0.45);
    float core = smoothstep(0.06, 0.0, radius);
    float flash = 0.65 + 0.35 * sin(time * 24.0);

    vec3 color = neon(fract(time * 0.9 + a * 0.159 + radius * 2.2));
    color *= 0.30 + 1.65 * max(blast, core) * flash;
    color += vec3(1.0, 0.95, 0.55) * core * flash;
    return vec4(clamp(color, 0.0, 1.0), 1.0);
}
