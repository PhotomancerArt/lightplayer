struct ControlMessage {
    uint id;
    uint seq;
};

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform ControlMessage held[1];

float circleMask(vec2 uv, vec2 center, float radius) {
    vec2 delta = uv - center;
    float d2 = dot(delta, delta);
    float inner = radius * 0.70;
    return 1.0 - smoothstep(inner * inner, radius * radius, d2);
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec3 color = vec3(0.012, 0.015, 0.020);

    if (held[0].id != 0u) {
        float pulse = 0.02 * sin(float(held[0].seq) * 1.7);
        float mask = circleMask(uv, vec2(0.5, 0.5), 0.26 + pulse);
        color = mix(color, vec3(0.1, 0.8, 0.95), mask);
    }

    return vec4(color, 1.0);
}
