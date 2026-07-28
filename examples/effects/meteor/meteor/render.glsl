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
vec3 drawMeteor(vec3 accum, int slot, vec2 uv) {
    if (meteors[slot].id == 0u) {
        return accum;
    }
    if (meteors[slot].intensity <= 0.0) {
        return accum;
    }
    float behind = meteors[slot].pos.x - uv.x;
    if (behind < 0.0) {
        behind = behind + 1.0;
    }
    float tail = exp(-behind * decay * 6.0);
    float lane = 1.0 - smoothstep(0.0, 0.45, abs(uv.y - meteors[slot].pos.y));
    return accum + meteors[slot].color * tail * lane * meteors[slot].intensity;
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec3 accum = vec3(0.0, 0.0, 0.0);
    accum = drawMeteor(accum, 0, uv);
    accum = drawMeteor(accum, 1, uv);
    accum = drawMeteor(accum, 2, uv);
    accum = drawMeteor(accum, 3, uv);
    return vec4(min(accum, vec3(1.0, 1.0, 1.0)), 1.0);
}
