// Peach body — a warm ember glow running along the wire, hot where the
// strand starts.
//
// A true 1D shader: it knows the strip and nothing else. The peach shape
// lives in the fixture's mapping document, which this shader never reads —
// that is exactly the 1D story. Lamp 0 is the bottom of the body, so the
// heat sits at 0 and falls off along the run.
//
// Closed form on one phasor, like the rest of the corpus: two ripples ride
// whole multiples of `phase`, so the animation is exact across the wrap and
// identical at any frame rate.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;
layout(binding = 2) uniform float glow;

vec4 render_1d(float pos) {
    // `pos` is a pixel coordinate; a 1D target reports (N, 1).
    float t = pos / outputSize.x;

    // Two ripples travelling back down the strand, so the embers breathe
    // instead of pulsing in unison.
    float ripple = 0.5
        + 0.28 * sin((t * 3.0 - phase * 4.0) * 6.2831853)
        + 0.22 * sin((t * 7.0 - phase * 9.0) * 6.2831853);

    // The ember core: bright at the wire's start, reaching `glow` of the way
    // along before it fades into the deep red the whole body keeps.
    float core = exp(-t / max(glow, 0.05));
    float level = core * (0.55 + 0.45 * ripple);

    vec3 ember = vec3(1.0, 0.45, 0.14) * level;
    vec3 body = vec3(0.30, 0.05, 0.02) * (1.0 - 0.6 * t);
    return vec4(ember + body, 1.0);
}
