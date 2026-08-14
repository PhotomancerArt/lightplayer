struct Meteor {
    uint id;
    vec2 pos;
    vec2 dir;
    float radius;
    vec3 color;
    float velocity;
    float intensity;
};

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform Meteor meteors[4];
layout(binding = 2) uniform float decay;

// One meteor's contribution: a bright head with an exponential tail behind
// it (the tail wraps with the head).
//
// The uniform array is indexed with CONSTANT indices at the call site and
// its fields arrive as scalars/vectors: a runtime index into a uniform
// struct array has no uniform element address to lower against.
// `examples/events/shader.glsl` carries the same shape for the same reason.
vec3 drawMeteor(vec3 accum, uint id, vec2 head, vec3 color, float intensity, vec2 uv) {
    if (id == 0u) {
        return accum;
    }
    if (intensity <= 0.0) {
        return accum;
    }
    float behind = head.x - uv.x;
    if (behind < 0.0) {
        behind = behind + 1.0;
    }
    float tail = exp(-behind * decay * 6.0);
    float lane = 1.0 - smoothstep(0.0, 0.45, abs(uv.y - head.y));
    return accum + color * tail * lane * intensity;
}

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec3 accum = vec3(0.0, 0.0, 0.0);
    accum = drawMeteor(accum, meteors[0].id, meteors[0].pos, meteors[0].color, meteors[0].intensity, uv);
    accum = drawMeteor(accum, meteors[1].id, meteors[1].pos, meteors[1].color, meteors[1].intensity, uv);
    accum = drawMeteor(accum, meteors[2].id, meteors[2].pos, meteors[2].color, meteors[2].intensity, uv);
    accum = drawMeteor(accum, meteors[3].id, meteors[3].pos, meteors[3].color, meteors[3].intensity, uv);
    return vec4(min(accum, vec3(1.0, 1.0, 1.0)), 1.0);
}
