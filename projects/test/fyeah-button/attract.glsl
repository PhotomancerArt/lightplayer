layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float wheelPhase;
// The three wheel moods are authored palettes now (attract.json's `palette`
// slot cycles them), which retires the hand-rolled trio, the bare-float
// switcher, and the crossfade this shader used to carry.
layout(binding = 2) uniform sampler2D palette;

float wheelDistance(float a, float b) {
    float d = abs(fract(a - b + 0.5) - 0.5);
    return d;
}

vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec2 p = uv - 0.5;
    float aspect = outputSize.x / outputSize.y;
    p.x *= aspect;

    float angle = atan(p.y, p.x) * 0.15915494 + 0.5;
    float radius = dot(p, p);
    float rim = smoothstep(0.0784, 0.1600, radius) * (1.0 - smoothstep(0.3136, 0.4900, radius));

    // One phasor is left: the wheel spins once per `wheelPhase` period. The
    // palette walk rides its own config-declared cycle instead.
    float wheel = fract(angle + wheelPhase);

    float slice = fract(wheel * 1.18);
    vec3 color = texture(palette, vec2(slice, 0.0)).rgb;

    float darkA = 1.0 - smoothstep(0.090, 0.145, wheelDistance(wheel, 0.18));
    float darkB = 1.0 - smoothstep(0.075, 0.125, wheelDistance(wheel, 0.68));
    float level = 1.0 - max(darkA, darkB);
    color *= rim * level * 1.18;
    return vec4(clamp(color, 0.0, 1.0), 1.0);
}
