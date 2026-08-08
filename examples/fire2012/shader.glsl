// Fire 2012 — a fire burning up the strip: white-hot at the base, tongues
// of flame climbing and cooling, sparks thrown clear of the body.
//
// Provenance (ADR 2026-07-29 + its 2026-08-01 addendum):
//   upstream repo: https://github.com/wled/WLED
//   file:          wled00/FX.cpp
//   function:      mode_fire_2012 ("Fire 2012", after Mark Kriegsman's
//                  FastLED Fire2012)
//   commit:        44e28f96e0af0c78cb1b902a45b6332dcacd10e0 (2024-10-15)
//   license:       MIT — Copyright (c) 2016 Christian Schwinne.
//                  Vendored at licenses/WLED-MIT.txt. WLED relicensed to
//                  EUPL on 2024-10-16, one commit later; this revision is
//                  MIT and is the only revision consulted.
//
// **The heat simulation is NOT ported.** Upstream Fire2012 keeps a byte of
// heat per cell and advances it every frame in four steps — cool, drift up
// and diffuse, randomly ignite sparks at the bottom, map heat to color. That
// is a stateful compute pipeline over a dense scalar array, which this
// engine cannot express today, so nothing here integrates anything. What is
// borrowed is the *look* and the *name*; the algorithm below is original.
//
// Instead of simulating the heat field, this shader writes down what the
// simulation converges to: an exponential heat gradient anchored at the base
// (the steady state of "cool everywhere, ignite at the bottom"), modulated
// by layered value noise scrolling upward (the drift-and-diffuse step, whose
// visible product is tongues of flame rising), with rare crests of the
// finest layer standing in for the spark die-roll. One `render_1d` call, no
// history, identical at 1 fps and 500 fps.
//
// Also unported: upstream's 2D mode blurs across virtual strips. Here the
// shader is honestly 1D and declares `OneD { in_2d: Default }`, so a 2D
// consumer decides how the flame column reaches a surface.

layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float rise;
layout(binding = 2) uniform float reach;
layout(binding = 3) uniform float sparks;
layout(binding = 4) uniform sampler2D palette;

// The ember field repeats every CELLS units of its own domain. Every scroll
// term below is a whole multiple of CELLS per phasor turn, so all three
// layers wrap together with `rise` and the fire never seams — the same
// whole-multiple discipline `examples/plasma` documents for its phasors.
const float CELLS = 32.0;

// One ember: a hashed value at an integer cell, wrapped into the repeat.
float ember(float cell) {
    return lpfn_random(mod(cell, CELLS), 0u);
}

// Value noise over the embers — hash the two neighbours, ease between them.
float flames(float x) {
    float i = floor(x);
    float f = fract(x);
    f = f * f * (3.0 - 2.0 * f);
    return mix(ember(i), ember(i + 1.0), f);
}

vec4 render_1d(float pos) {
    // `pos` is a pixel coordinate; a 1D target reports (N, 1). t = 0 is the
    // base of the fire, t = 1 the far end the flame is climbing toward.
    float t = pos / outputSize.x;

    // Three layers of embers climbing the strip. Coarse and slow is the body
    // of the flame; fine and fast is the flicker. Upstream gets this spread
    // of scales for free because `heat[k] = (heat[k-1] + 2*heat[k-2]) / 3`
    // is a diffusion — big features survive the trip up, small ones do not.
    //
    // Subtracting the scroll term makes the layers climb toward t = 1. At the
    // authored 16 s period they travel 40, 55 and 62 lamps per second on this
    // 120-lamp strip, which is the range upstream lands in when it shifts its
    // heat array one cell per frame at 30-60 fps.
    float body = flames(t * 6.0 - rise * CELLS);
    float lick = flames(t * 13.0 - rise * CELLS * 3.0);
    float glint = flames(t * 27.0 - rise * CELLS * 7.0);
    float turbulence = 0.55 * body + 0.30 * lick + 0.15 * glint;

    // The heat gradient: hottest at the base, cooling as it climbs. This is
    // what upstream's cool-and-shift loop settles into; `reach` is its scale
    // height, and plays the role of WLED's Cooling slider inverted (more
    // reach = less cooling = a taller fire).
    float climb = exp(-t / max(reach, 0.05));

    // Multiply rather than add, so the tongues thin out with height instead
    // of the whole strip flickering as one.
    float heat = climb * mix(0.25, 1.35, turbulence);

    // Upstream's ignition area: the bottom tenth is held above black so the
    // fire always has a bed to burn from.
    float ignition = clamp(1.0 - t * 10.0, 0.0, 1.0);
    heat = max(heat, ignition * 0.6);

    // Sparks. WLED rolls a die per frame against its Spark-rate slider and
    // injects heat near the bottom; the closed-form equivalent is the crests
    // of the fastest layer raised to a high power, so only the rare peaks
    // survive, carried by a slower falloff than the body — which is what
    // puts them above the flame instead of inside it.
    float thrown = exp(-t / max(reach * 2.2, 0.05));
    heat = heat + sparks * pow(glint, 6.0) * thrown;

    // Step 4 unchanged in spirit: heat is the palette index, black-body
    // order — black, dull red, orange, yellow, white hot.
    return vec4(texture(palette, vec2(clamp(heat, 0.0, 1.0), 0.0)).rgb, 1.0);
}
