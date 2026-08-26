layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;
layout(binding = 2) uniform float bands;
layout(binding = 3) uniform float tilt;
layout(binding = 4) uniform sampler2D palette;

// Light travelling across the sign, in three moves you can watch one at a
// time: pick a direction, count how many color bands fit along it, then
// slide them.
//
// `tilt` is a turn, not degrees — 0 sends the light left to right, 0.25
// sends it top to bottom, 1 is all the way back around. `bands` is how many
// palette repeats fit across the sign. `phase` is a 0..1 phasor, so adding
// it to the palette coordinate scrolls the whole ramp exactly once per
// cycle; the strip samples wrap=repeat, so it never clamps.
vec4 render_2d(vec2 pos) {
    vec2 uv = pos / outputSize;
    float angle = tilt * 6.2831853;
    vec2 heading = vec2(cos(angle), sin(angle));
    float travel = dot(uv - vec2(0.5, 0.5), heading);

    // A shallow ripple across the travel direction, at a whole multiple of
    // the base cycle, so the bands breathe instead of marching flat. Whole
    // multiples are what keep the wrap seamless.
    float ripple = 0.04 * sin((travel * 2.0 - phase * 3.0) * 6.2831853);

    return vec4(texture(palette, vec2(travel * bands + phase + ripple, 0.0)).rgb, 1.0);
}
