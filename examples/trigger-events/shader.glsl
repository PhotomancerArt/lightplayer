struct ControlMessage {
    uint id;
    uint seq;
};

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform ControlMessage events[8];

float circleMask(vec2 uv, vec2 center, float radius) {
    vec2 delta = uv - center;
    float d2 = dot(delta, delta);
    float inner = radius * 0.72;
    return 1.0 - smoothstep(inner * inner, radius * radius, d2);
}

vec3 eventColor(uint id, uint seq) {
    float hue = mod(float(id * 37u + seq * 11u), 97.0) / 97.0;
    return 0.55 + 0.45 * cos(6.2831853 * (hue + vec3(0.00, 0.33, 0.67)));
}

vec3 drawEvent(vec3 accum, int slot, uint id, uint seq, vec2 uv) {
    if (id == 0u) {
        return accum;
    }
    float col = float(slot % 4);
    float row = float(slot / 4);
    vec2 center = vec2(0.18 + col * 0.21, 0.34 + row * 0.32);
    float pulse = 0.08 + 0.01 * float(seq % 5u);
    float mask = circleMask(uv, center, pulse);
    return max(accum, eventColor(id, seq) * mask);
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec3 color = vec3(0.015, 0.018, 0.025);

    color = drawEvent(color, 0, events[0].id, events[0].seq, uv);
    color = drawEvent(color, 1, events[1].id, events[1].seq, uv);
    color = drawEvent(color, 2, events[2].id, events[2].seq, uv);
    color = drawEvent(color, 3, events[3].id, events[3].seq, uv);
    color = drawEvent(color, 4, events[4].id, events[4].seq, uv);
    color = drawEvent(color, 5, events[5].id, events[5].seq, uv);
    color = drawEvent(color, 6, events[6].id, events[6].seq, uv);
    color = drawEvent(color, 7, events[7].id, events[7].seq, uv);

    return vec4(color, 1.0);
}
