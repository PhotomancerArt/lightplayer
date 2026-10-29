layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;
layout(binding = 2) uniform float scale;
layout(binding = 3) uniform sampler2D palette;

// Classic plasma: three folded sine fields plus a radial term, read
// through a palette.
//
// Every field used to advance at its own multiple of 0.01 Hz, so one phasor
// carries the whole animation: `phase` is the 0.01 Hz base cycle and each
// field rides a whole-number multiple of it. Whole multiples are what keeps
// the rewrite exact — the wrap they skip is a whole number of sine periods.
vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    float v = sin((uv.x * scale + phase * 13.0) * 6.2831853)
        + sin((uv.y * scale + phase * 9.0) * 6.2831853)
        + sin(((uv.x + uv.y) * scale * 0.5 + phase * 11.0) * 6.2831853)
        + sin((length(uv - vec2(0.5, 0.5)) * scale + phase * 15.0) * 6.2831853);
    // `hue` runs well past [0,1) on purpose — the strip samples wrap=repeat,
    // so the ramp scrolls instead of clamping.
    float hue = v * 0.125 + phase * 5.0;
    return vec4(texture(palette, vec2(hue, 0.0)).rgb, 1.0);
}
