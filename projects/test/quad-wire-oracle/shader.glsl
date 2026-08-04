// Deterministic, time-invariant test pattern.
//
// This project exists to be rendered on two targets and compared BYTE FOR
// BYTE, so it must consume no clock: every frame is identical, and the only
// remaining variable between a host render and a device render is the shader
// compiler/executor under test.
//
// It is not decorative. It reaches for the operations an on-device JIT is most
// likely to get subtly wrong: a divide, a square root, a two-argument arctan,
// transcendentals through the builtin table, a data-dependent branch, and a
// helper call that cannot be folded into its caller. A pattern of flat ramps
// would render "plausibly" through a half-broken backend.

layout(binding = 0) uniform vec2 outputSize;

// Data-dependent branch. Triangle wave over one unit of x.
float ridge(float x) {
    float f = fract(x);
    if (f < 0.5) {
        return f * 2.0;
    }
    return (1.0 - f) * 2.0;
}

// Three cosines out of the builtin table, kept inside [0,1].
vec3 palette(float t) {
    return clamp(
        vec3(
            0.5 + 0.5 * cos(6.2831853 * (t + 0.00)),
            0.5 + 0.5 * cos(6.2831853 * (t + 0.33)),
            0.5 + 0.5 * cos(6.2831853 * (t + 0.67))
        ),
        0.0,
        1.0
    );
}

vec4 render(vec2 pos) {
    vec2 uv = pos / outputSize;
    vec2 c = uv - vec2(0.5, 0.5);

    float r = sqrt(c.x * c.x + c.y * c.y);
    float a = atan(c.y, c.x) / 6.2831853 + 0.5;

    float band = ridge(r * 4.0);
    float swirl = fract(a * 3.0 + r * 2.0);
    float t = mix(band, swirl, 0.5);

    float v = smoothstep(0.0, 1.0, 1.0 - r);
    return vec4(palette(t) * v, 1.0);
}
