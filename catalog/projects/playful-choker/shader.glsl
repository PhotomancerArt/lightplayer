// PLAYFUL choker: one smooth noise field sliding along the word, colored by a
// palette that cycles (sunset, lagoon, candy, meadow, ember), with a finer
// octave rippling the brightness so the letters twinkle rather than just fade.
//
// Coordinates are normalised to the piece's height, so `p.x` runs 0..~3.7
// across the word and `p.y` 0..1 top to bottom whatever the render size.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float driftPhase;
layout(binding = 3) uniform float shimmerPhase;
layout(binding = 4) uniform float scale;
layout(binding = 5) uniform sampler2D palette;

const float TAU = 6.2831853;

vec4 render_2d(vec2 pos) {
    vec2 p = pos / outputSize.y;
    float drift = sin(TAU * driftPhase);

    // Wide, slow blobs: about one per two letters at scale 1, scrolling left
    // and wandering up and down with the drift phasor.
    vec2 gradient;
    float broad = lpfn_psrdnoise(
        p * (0.55 * scale) + vec2(time * 0.045, drift * 0.30),
        vec2(0.0),
        time * 0.07,
        gradient,
        0u
    );

    // Fine octave: per-letter sparkle, scrolling the other way.
    float fine = lpfn_psrdnoise(
        p * (1.9 * scale) - vec2(time * 0.09, 0.0),
        vec2(0.0),
        time * 0.23,
        gradient,
        0u
    );

    // Palette position: the broad field picks the hue, the x position adds a
    // gentle rainbow tilt along the word. The strip samples wrap=repeat, so
    // the value may run past 1 and the ramp just scrolls.
    float hue = broad * 0.45 + fine * 0.12 + p.x * 0.10 + time * 0.02;

    // Brightness: never black (the word must stay readable), rippling with
    // the fine octave and the shimmer phasor.
    float ripple = 0.5 + 0.5 * sin(TAU * (shimmerPhase + fine * 0.35));
    float lum = 0.45 + 0.55 * ripple;

    vec3 color = texture(palette, vec2(hue, 0.0)).rgb * lum;
    return vec4(clamp(color, 0.0, 1.0), 1.0);
}
