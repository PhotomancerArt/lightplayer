// PLAYFUL choker: bands of color sweeping along the word, with sharp fronts
// between them, all at one perceived brightness.
//
// Every lamp is the same Oklch lightness and chroma; only the hue moves. A
// brightness wave makes the letters read unevenly, and a hue wave does not.
// The fronts are the point: a crisp edge travelling across the letters shows
// the mapping off in a way a soft blend cannot.
//
// Coordinates are normalised to the piece's height, so `p.x` runs 0..~3.7
// across the word and `p.y` 0..1 top to bottom whatever the render size.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float driftPhase;
layout(binding = 3) uniform float scale;
layout(binding = 4) uniform float frontWidth;

const float TAU = 6.2831853;

// Oklch lightness and chroma for every lamp. At L 0.75 every hue reaches
// chroma 0.127 inside sRGB, so 0.125 is as vivid as the circle gets without
// any hue clipping (and clipping is what would change the brightness).
const float LIGHTNESS = 0.75;
const float CHROMA = 0.125;

// Hue distance between neighbouring bands, in turns: the golden fraction, so
// consecutive bands never land on the same color and each front is a big
// jump.
const float HUE_STEP = 0.382;

vec4 render_2d(vec2 pos) {
    vec2 p = pos / outputSize.y;

    // Fronts run mostly across the word, leaning up to ~35° either way with
    // the drift phasor so they cut the letters diagonally too.
    float lean = 0.6 * sin(TAU * driftPhase);
    vec2 dir = vec2(cos(lean), sin(lean));

    // A slow noise field bends the fronts so they are not ruler-straight.
    vec2 gradient;
    float warp = lpfn_psrdnoise(
        p * 0.8 + vec2(0.0, time * 0.05),
        vec2(0.0),
        time * 0.1,
        gradient,
        0u
    );

    // Band coordinate: one unit per band, scrolling along the word. A front
    // passes a given letter every ~5.5 s at scale 1.
    float u = dot(p, dir) * (0.9 * scale) - time * 0.18 + warp * 0.35;

    // Constant hue inside a band, then a quick smoothstep up to the next
    // band's hue over the last `frontWidth` of it: the front.
    float band = floor(u);
    float front = smoothstep(1.0 - frontWidth, 1.0, fract(u));
    float hue = (band + front) * HUE_STEP + warp * 0.03;

    vec3 color = lpfn_oklch2rgb(vec3(LIGHTNESS, CHROMA, hue));
    return vec4(clamp(color, 0.0, 1.0), 1.0);
}
