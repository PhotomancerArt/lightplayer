layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float progress;

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

    float t = clamp(progress, 0.0, 1.0);
    float decay = pow(1.0 - t, 1.55);
    float envelope = mix(0.10, 1.0, decay);
    float ember = pow(1.0 - t, 0.45);

    float radius = dot(p, p);
    float a = atan(p.y, p.x);
    float spokes = sin(a * 12.0 + time * mix(12.0, 7.0, t));
    float rings = sin(radius * mix(64.0, 42.0, t) - time * mix(18.0, 8.0, t));
    float shards = lpfn_fbm(p * mix(14.0, 8.0, t) + vec2(time * 0.55, -time * 0.32), 2, 0u);
    float blast = smoothstep(0.75, 1.0, spokes * 0.55 + rings * 0.45);
    float core = smoothstep(0.06, 0.0, radius);
    float sparks = smoothstep(0.56, 0.92, shards + blast * 0.45);
    float flash = 0.70 + 0.30 * sin(time * mix(24.0, 13.0, t));

    vec3 color = neon(fract(time * 0.62 + a * 0.159 + radius * 2.2 + t * 0.18));
    color *= 0.12 + (0.45 + 1.85 * max(blast, core)) * flash * envelope;
    color += vec3(1.0, 0.95, 0.55) * core * flash * mix(0.30, 1.25, envelope);
    color += vec3(1.0, 0.92, 0.72) * sparks * flash * envelope * ember * 0.85;
    return vec4(clamp(color, 0.0, 1.0), 1.0);
}
