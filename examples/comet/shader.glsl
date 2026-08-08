// Comet — a bright head sweeping the strip with a trail fading behind it.
//
// Provenance (ADR 2026-07-29 + its 2026-08-01 addendum):
//   upstream repo: https://github.com/wled/WLED
//   file:          wled00/FX.cpp
//   function:      mode_comet ("Lighthouse")
//   commit:        44e28f96e0af0c78cb1b902a45b6332dcacd10e0 (2024-10-15)
//   license:       MIT — Copyright (c) 2016 Christian Schwinne.
//                  Vendored at licenses/WLED-MIT.txt. WLED relicensed to
//                  EUPL on 2024-10-16, one commit later; this revision is
//                  MIT and is the only revision consulted.
//
// Re-authored, not transliterated. WLED walks an integer head index along
// the segment and calls `fade_out` on the whole strip once per frame, so
// its tail is a side effect of repeated fading — the shape depends on the
// frame rate. Here the tail is closed form: the wrapped distance behind the
// head, read through an exponential falloff. One `render_1d` call, no
// history, identical at 1 fps and 500 fps.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float sweep;
layout(binding = 2) uniform float tail;
layout(binding = 3) uniform sampler2D palette;

vec4 render_1d(float pos) {
    // `pos` is a pixel coordinate; a 1D target reports (N, 1).
    float t = pos / outputSize.x;

    // Distance BEHIND the head, wrapped: to the trail the strip is a ring
    // even when the fixture is a line, which is what makes the head's
    // re-entry at 0 continuous instead of a jump.
    float behind = sweep - t;
    if (behind < 0.0) {
        behind = behind + 1.0;
    }
    float trail = exp(-behind / max(tail, 0.002));

    // WLED indexes the palette by PIXEL, not by energy, so the comet takes
    // whatever color the strip holds where it happens to be — the ramp is a
    // property of the strip, and the comet lights it.
    return vec4(texture(palette, vec2(t, 0.0)).rgb * trail, 1.0);
}
