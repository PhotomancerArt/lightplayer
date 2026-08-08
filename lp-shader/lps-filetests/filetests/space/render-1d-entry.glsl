// test run

// The 1D entry (dimensionality plan D19): a shader declaring `OneD` defines
// `vec4 render_1d(float pos)` instead of `render_2d(vec2)`.
//
// To the frontends and the backends the entry is a plain function of one
// float — which is the point of pinning it here. Every target must evaluate
// it identically, so the only places dimensionality actually lives are the
// synthesised CPU wrappers (`__render_texture_*` / `__render_samples_*`) and
// the GPU splice, both covered by their own tests. If this file ever
// diverges per target, the entry stopped being an ordinary function
// somewhere it should not have.
//
// `pos` is a **pixel** coordinate in the same frame currency as 2D, so the
// authored code normalizes against `outputSize.x` (a 1D target reports
// `(N, 1)`).

layout(binding = 0) uniform vec2 outputSize;

vec4 render_1d(float pos) {
    float t = pos / outputSize.x;
    return vec4(t, 1.0 - t, 0.25, 1.0);
}

// set_uniform: outputSize = vec2(8.0, 1.0)
// run: render_1d(0.5) ~= vec4(0.0625, 0.9375, 0.25, 1.0)

// set_uniform: outputSize = vec2(8.0, 1.0)
// run: render_1d(4.5) ~= vec4(0.5625, 0.4375, 0.25, 1.0)

// set_uniform: outputSize = vec2(8.0, 1.0)
// run: render_1d(0.0) ~= vec4(0.0, 1.0, 0.25, 1.0)

// A 1D entry is an ordinary caller too: helpers, control flow and builtins
// work exactly as they do from a 2D entry.
float ramp(float t) {
    if (t < 0.5) {
        return t * 2.0;
    }
    return 2.0 - t * 2.0;
}

vec4 render_1d_helper_probe(float pos) {
    return vec4(ramp(pos), 0.0, 0.0, 1.0);
}

// run: render_1d_helper_probe(0.25) ~= vec4(0.5, 0.0, 0.0, 1.0)
// run: render_1d_helper_probe(0.75) ~= vec4(0.5, 0.0, 0.0, 1.0)
