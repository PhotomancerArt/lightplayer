// Palette waves — a palette ramp travelling the strip under a slow
// brightness swell.
//
// Provenance (ADR 2026-07-29 + its 2026-08-01 addendum):
//   upstream repo: https://github.com/wled/WLED
//   file:          wled00/FX.cpp
//   function:      mode_colorwaves ("Colorwaves")
//   commit:        44e28f96e0af0c78cb1b902a45b6332dcacd10e0 (2024-10-15)
//   license:       MIT — Copyright (c) 2016 Christian Schwinne.
//                  Vendored at licenses/WLED-MIT.txt. WLED relicensed to
//                  EUPL on 2024-10-16, one commit later; this revision is
//                  MIT and is the only revision consulted.
//
// Re-authored, not transliterated. WLED carries two 16-bit accumulators
// across frames (`sHue16`, `sPseudotime`) advanced by beat-driven
// increments, so its motion is a function of how often the effect ran. Both
// are pure functions of time, so here they are one phasor and whole
// multiples of it — the same discipline `examples/plasma` documents: a
// whole multiple wraps on a whole number of cycles, so the rewrite is exact
// at the phasor's own wrap.
//
// This shader declares `OneD { in_2d: Radial }`: it is written along a
// strip, and when a 2D consumer asks for it the declared answer turns the
// travelling ramp into rings.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float phase;
layout(binding = 2) uniform float span;
layout(binding = 3) uniform float depth;
layout(binding = 4) uniform sampler2D palette;

// WLED folds the top bits of its 16-bit hue accumulator so the ramp
// ping-pongs instead of snapping back at the wrap (`h16_128 & 0x100`
// selects between the value and its complement). A triangle wave IS that
// fold, and it is the reason colorwaves reads as waves rather than as a
// scrolling rainbow.
float fold(float x) {
    return abs(fract(x * 0.5) * 2.0 - 1.0);
}

vec4 render_1d(float pos) {
    // `pos` is a pixel coordinate; a 1D target reports (N, 1).
    float t = pos / outputSize.x;

    // Hue ramp along the strip, drifting as a whole.
    float hue = fold(t * span + phase * 3.0);

    // Brightness swell travelling the other way. WLED squares its 16-bit
    // sine before scaling it by `brightdepth`, which is what keeps the dark
    // troughs wide and the bright crests narrow; `depth` is that
    // beat-driven `brightdepth`, here an authored control.
    float wave = 0.5 + 0.5 * sin((t * 2.0 - phase * 7.0) * 6.2831853);
    float bright = mix(1.0 - depth, 1.0, wave * wave);

    return vec4(texture(palette, vec2(hue, 0.0)).rgb * bright, 1.0);
}
