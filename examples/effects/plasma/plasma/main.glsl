layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float speed;
layout(binding = 3) uniform float scale;

// Classic plasma: three folded sine fields plus a radial term, hue-cycled.
vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    float t = time * speed;
    float v = sin((uv.x * scale + t * 0.13) * 6.2831853)
        + sin((uv.y * scale + t * 0.09) * 6.2831853)
        + sin(((uv.x + uv.y) * scale * 0.5 + t * 0.11) * 6.2831853)
        + sin((length(uv - vec2(0.5, 0.5)) * scale + t * 0.15) * 6.2831853);
    float hue = v * 0.125 + t * 0.05;
    vec3 rgb = 0.5 + 0.5 * cos(6.2831853 * (hue + vec3(0.0, 0.33, 0.67)));
    return vec4(rgb, 1.0);
}
