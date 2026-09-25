# Ideas: prior art for the pattern library

A sweep of LED-pattern and shader prior art, for pattern authors (human or
agent) to draw from. **This is an idea list, not a spec.** An entry names a
thing worth trying on LightPlayer's pattern-space rule (`vision.md` in
`2026-09-23-2356-pattern-library`, D8): the shader sees the lamps, centred,
long side −1…1, y up, with `extent`/`pitch`/count alongside `pos`. Nothing
here is a finished pattern, a contract-compliant knob set, or code to paste
in.

## How to use this file

1. Pick an entry (or a handful in one family) whose "what it looks like"
   sentence sounds good on the piece you're targeting.
2. Reimplement it from scratch against the pattern-space rule — never port
   pixel-indexed logic. See the licence rule below before looking at any
   reference source's code.
3. Give it 2–4 knobs, palette-driven where it makes sense, and check it
   against the pattern contract in `vision.md`.
4. Entries marked "needs structure" don't work on a generic 2D plane (they
   assume a grid, a path, or a fixed layout) — useful ideas, out of scope
   for the library today.

## Licence rules (from `AGENTS.md` — read it in full before touching any source)

1. **No code from here goes into the repo.** This file holds names,
   one-sentence descriptions, and links — never quotes, never pasted
   snippets, never transliterations.
2. **`idea-only`** sources (WLED, Pixelblaze, Madrix, Resolume, Shadertoy,
   The Book of Shaders, Inigo Quilez's articles, Chromatik/LX) are
   behavioural references only: look at what a pattern *does*, then
   implement it independently from primary technique descriptions (noise
   functions, SDFs, standard graphics maths). Never read their source with
   intent to adapt it.
3. **`code-ok (licence)`** sources (FastLED: MIT; Adafruit's
   CircuitPython/Arduino LED code: MIT) may be read for technique, but this
   file still carries no code — a future PR that leans on one of these
   states the licence and keeps provenance, per the ADR.
4. **AGPL projects are never a source here.** None turned up in this sweep;
   if one ever does, it's `idea-only` at most, same as GPL tooling in
   `AGENTS.md`.
5. **If unsure whether a source is safe to look at: ask, don't copy.**
6. An entry is a *starting point*. LightPlayer's pattern-space rule, Q32
   fixed-point, and the shared-knob bus (`vision.md` D14) mean every
   reimplementation looks different from its source in the ways that matter.

## What LightPlayer can do that others can't

Firmware effect libraries (WLED, FastLED, Pixelblaze) run on 8-bit
lookup-table noise, integer sine tables, and per-pixel loops with no real
continuous math — they're built for AVR/ESP8266-class budgets. LightPlayer
JITs real GLSL, so these are cheap here and rare (or approximated) there:

- **Domain warping** — feeding one noise field's output as another's input
  coordinate (Inigo Quilez's technique); produces marbled, veined, or
  vein-like flow that a LUT-noise engine can only fake with layered scroll
  offsets.
- **Signed distance fields (SDFs)** — exact analytic distance to a shape
  (circle, rounded box, hex), so edges anti-alias for free and glow falloff
  is a real function of distance, not a blur pass.
- **Smooth kaleidoscope folds** — continuous angular fold-and-mirror
  (`angle = abs(mod(angle, seg) - seg/2)`) at arbitrary segment counts, not
  the fixed 2D-matrix mirror tricks WLED's `Drift`/`Frizzles` use.
- **True per-pixel Perlin/simplex/value noise with continuous derivatives**
  — smooth gradients and normal-like shading, not WLED's 8-bit `inoise8`
  stepping.
- **Analytic rotation and scale as free per-pixel operations** — every
  pattern can rotate or zoom around the piece's own centre without a
  lookup table or fixed 16×16 grid.
- **Fractal iteration (Julia/Mandelbrot-style escape-time colouring)** —
  a handful of complex-plane iterations per pixel, impractical on 8-bit
  firmware loops but routine in a JIT'd shader.

## Yona's seed list (start here)

These eight are the seed list from `vision.md` D5 — build these first; they
anchor each family's tone.

- ★ Fields: **noise, soft** — a slow, blurred drift of light and dark with no
  hard edges.
- ★ Fields: **noise, hard** — the same drift, thresholded into flat regions
  with sharp boundaries.
- ★ Fields: **noise, veins** — thin bright lines along a noise field's ridges,
  like cracked ice or veins.
- ★ Gradients: **linear gradient — motion** — a colour ramp that slides
  steadily across the piece.
- ★ Gradients: **linear gradient — repeat** — the same ramp tiled so it
  cycles more than once across the piece.
- ★ Gradients: **linear gradient — rotation** — the ramp's axis spins slowly
  around the piece's centre.
- ★ Gradients: **radial gradient — detail** — rings from the centre, spacing
  set by the detail knob.
- ★ Gradients: **radial gradient — scrolling** — the same rings, flowing
  outward (or inward) over time.
- ★ Fronts: **colour wipes** — one colour replaces another as a straight
  edge sweeps across the piece.
- ★ Fronts: **scanner sweep** — a single bright band sweeps back and forth,
  Cylon/KITT-style.
- ★ Points: **twinkle on a background colour** — sparse points fade in and
  out at random over a steady base colour.

---

## Fields — a texture drifts

| Name | What it looks like | 2D? | Source | Licence | Wearable? |
|---|---|---|---|---|---|
| ★ Noise, soft | Slow blurred light/dark drift, no hard edges | yes | Seed list D5 | — | yes |
| ★ Noise, hard | Same drift, thresholded into flat regions | yes | Seed list D5 | — | yes |
| ★ Noise, veins | Bright thin lines along noise ridges | yes | Seed list D5 | — | yes |
| Plasma | Classic layered-sine colour plasma (exists in `catalog/patterns/plasma`) | yes | [WLED: Plasma](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Fill Noise | A single noise field mapped straight to colour, whole strip | yes | [WLED: Fill Noise](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Noise Pal | Slow peaceful noise with a shifting palette underneath | yes | [WLED: Noise Pal](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Noise2D | Two-dimensional Perlin noise field, no palette cycling | yes | [WLED: Noise2D](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Perlin Move | Perlin noise field driving position/brightness, not colour | yes | [WLED: Perlin Move](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Firenoise | A fire palette read through a Perlin noise field | yes | [WLED: Firenoise](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Hiphotic | A fast-moving multi-layer plasma-like field | yes | [WLED: Hiphotic](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe (busy) |
| PS Fuzzy Noise | An organic, softly flowing 2D noise field (WLED's particle system) | yes | [WLED: PS Fuzzy Noise](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Polar Lights / Aurora | Drifting vertical bands of colour like the aurora borealis | yes | [WLED: Polar Lights](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Lake | A calm palette moving as a slow waving field | yes | [WLED: Lake](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Colour Clouds | Soft evolving blobs of colour, slower and blurrier than plasma | yes | [WLED: Color Clouds](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Pacifica field | Ocean-wave layered sine/noise field (as a static-camera field, not a sim) | yes | [FastLED: Pacifica](https://fastled.io/docs/examples.html) | code-ok (MIT) | yes |
| Domain-warped noise ✦ | A noise field warped by a second noise field: marbled, flowing veins | yes | [iq: Domain warping](https://iquilezles.org/articles/warp/) | idea-only | yes |
| Value-noise marble ✦ | Smooth marble/stone veining from layered value noise | yes | [Book of Shaders: Cellular Noise](https://thebookofshaders.com/12/) | idea-only | yes |
| Fractional Brownian motion clouds ✦ | Multi-octave noise summed for cloud-like soft texture | yes | [Book of Shaders: fBm](https://thebookofshaders.com/13/) | idea-only | yes |
| Voronoi cells | Sharp-edged cellular regions that drift and reshape | yes | [Book of Shaders: Cellular Noise](https://thebookofshaders.com/12/) | idea-only | maybe |
| Metaballs | Blobby shapes that merge and split as they drift | yes | [WLED: Metaballs](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| GEQ Nova field | A radial field that pulses outward from spectral energy (reinterpret without audio) | yes | [WLED: PS GEQ Nova](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Squared Swirl | Boxes swirling and drifting across the field | yes | [WLED: Squared Swirl](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Frizzles field | Chaotic short trailing strokes moving across the field | yes | [WLED: Frizzles](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | no (busy) |

## Gradients — a colour ramp across space

| Name | What it looks like | 2D? | Source | Licence | Wearable? |
|---|---|---|---|---|---|
| ★ Linear gradient — motion | A ramp slides steadily across the piece | 1D and 2D | Seed list D5 | — | yes |
| ★ Linear gradient — repeat | The ramp tiles, cycling more than once | 1D and 2D | Seed list D5 | — | yes |
| ★ Linear gradient — rotation | The ramp's axis spins slowly about the centre | yes | Seed list D5 | — | yes |
| ★ Radial gradient — detail | Rings from centre, spacing set by detail | yes | Seed list D5 | — | yes |
| ★ Radial gradient — scrolling | Rings flow outward or inward over time | yes | Seed list D5 | — | yes |
| Colorwaves | A palette read through a slow travelling phase wave | yes | [FastLED: Pride2015 / Colorwaves](https://fastled.io/docs/examples.html) | code-ok (MIT) | yes |
| Pride2015 | Rainbow with slowly shifting brightness and hue drift | yes | [FastLED: Pride2015](https://fastled.io/docs/examples.html) | code-ok (MIT) | yes |
| Gradient (saturation) | A saturation gradient of one hue travels along the piece | yes | [WLED: Gradient](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Colorloop | The whole surface cycles together through rainbow hues | yes | [WLED: Colorloop](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Flow | A blend between a palette wash and a moving spot | yes | [WLED: Flow](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Sine (phased) | Controllable sine wave(s) read as brightness/colour phase | yes | [WLED: Sine / Phased](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Spiral gradient ✦ | A ramp that winds outward from centre like a spiral arm | yes | [WLED: Drift Rose](https://kno.wled.ge/features/effects/) (analogous) | idea-only (EUPL) | yes |
| Conic / angular gradient ✦ | A ramp that sweeps by angle around the centre, like a colour wheel | yes | [Book of Shaders: Shapes](https://thebookofshaders.com/07/) | idea-only | yes |
| Kaleidoscope fold ✦ | A small wedge of pattern mirrored and repeated around the centre | yes | [WLED: Drift](https://kno.wled.ge/features/effects/), [Shadertoy kaleidoscope studies](https://www.shadertoy.com/results?query=kaleidoscope) | idea-only | maybe (busy) |
| DNA spiral gradient | Two interleaved colour ramps twisting around each other | yes (as abstract twist, not the literal helix) | [WLED: DNA Spiral](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Distortion waves | A gradient distorted by a slow sine field into a psychedelic ripple | yes | [WLED: Distortion Waves](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| SDF glow ramp ✦ | A radial-distance glow from an analytic shape (circle, rounded box) rather than pixel distance | yes | [iq: 2D distance functions](https://iquilezles.org/articles/distfunctions2d/) | idea-only | yes |
| Julia-set colouring ✦ | Escape-time fractal colouring animated by drifting the constant | yes | [WLED: Julia](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Tri-colour solid pattern | Three-band repeating colour pattern, band width tunable | 1D and 2D | [WLED: Solid Pattern Tri](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |

## Fronts — an edge or band crosses the piece

| Name | What it looks like | 2D? | Source | Licence | Wearable? |
|---|---|---|---|---|---|
| ★ Colour wipes | One colour replaces another as a straight edge sweeps across | yes | Seed list D5 | — | yes |
| ★ Scanner sweep | A single bright band sweeps back and forth (Cylon/KITT) | 1D and 2D | Seed list D5; [FastLED: Cylon](https://fastled.io/docs/examples.html) | code-ok (MIT) | yes |
| Wipe Random | Colour-wipe edge, but each sweep picks a random colour | yes | [WLED: Wipe Random](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Theater chase | A pattern of lit/unlit bands scrolls steadily | 1D and 2D | [WLED: Theater](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Running sine | Sine-shaped brightness waves scroll along the piece | yes | [WLED: Running](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Saw wave | A sawtooth brightness ramp scrolls, sharp reset edge | yes | [WLED: Saw](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Oscillate | A band of colour moves back and forth between the piece's ends | yes | [WLED: Oscillate](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Lighthouse | A single dot sweeps end to end leaving a fading trail | yes | [WLED: Lighthouse](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Ripple rings | Concentric rings expand outward from a triggered point | yes | [WLED: Ripple](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Ripple Rainbow | Expanding rings, each a different rainbow hue | yes | [WLED: Ripple Rainbow](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Rain | Blobs of colour fall steadily like rainfall | yes | [WLED: Rain](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Multi comet | Several trailing comets cross the piece at once | yes | [WLED: Multi Comet](https://kno.wled.ge/features/effects/), `catalog/patterns/comet` | idea-only (EUPL) | yes |
| Meteor / meteor smooth | A bright head with a smoothly decaying trail crosses the piece (exists: `catalog/patterns/meteor`) | 1D and 2D | [WLED: Meteor](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| ICU | Two "eyes" sweep opposite edges of the piece | yes | [WLED: ICU](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Chase (two-dot) | Two lit points chase each other across a coloured background | 1D and 2D | [WLED: Chase](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Two dots | Two sweeping bands of colour cross and pass through each other | yes | [WLED: Two Dots](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Loading bar | A sawtooth-edged fill sweeps across like a progress bar | yes | [WLED: Loading](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Drip | A falling drop that splashes and ripples on landing | yes | [WLED: Drip](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Washing machine | A band spins, slows, reverses, like a washing-machine drum | yes | [WLED: Washing Machine](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Sinelon | A single glowing dot moves back and forth, trail fading behind it | 1D and 2D | [FastLED: DemoReel100 (sinelon)](https://github.com/FastLED/FastLED/blob/master/examples/DemoReel100/DemoReel100.ino) | code-ok (MIT) | yes |
| Waving cell | A band that ripples through like the stadium "wave" | yes | [WLED: Waving Cell](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |

## Points — sparks in time

| Name | What it looks like | 2D? | Source | Licence | Wearable? |
|---|---|---|---|---|---|
| ★ Twinkle on a background colour | Sparse points fade in and out over a steady base colour | yes | Seed list D5 | — | yes |
| Sparkle | Single random points briefly flash, otherwise dark | yes | [WLED: Sparkle](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Sparkle+ | Several random points flash at once | yes | [WLED: Sparkle+](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Sparkle Dark | Lit points randomly wink off, rather than dark points flashing on | yes | [WLED: Sparkle Dark](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Glitter | A rainbow base wash with sharp white sparkle points over it | yes | [WLED: Glitter](https://kno.wled.ge/features/effects/), [FastLED: `addGlitter`](https://fastled.io/docs/examples.html) | code-ok (MIT) | yes |
| Colortwinkles | Points light up in random colours, then fade | yes | [WLED: Colortwinkles](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Twinklefox | Gentle twinkling with a slow fade-out, forest-string feel | yes | [FastLED: TwinkleFox](https://fastled.io/docs/examples.html) | code-ok (MIT) | yes |
| Twinklecat | Twinkling with a fast fade-in and slow fade-out | yes | [WLED: Twinklecat](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Fairylight | Warm Christmas-light-style gentle twinkle | yes | [WLED: Fairy](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Fairytwinkle | Twinkles that start fully lit and fade, instead of fading in | yes | [WLED: Fairytwinkle](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Fireflies ✦ | Warm points that drift slowly and blink, like real fireflies (not fixed-position twinkle) | yes | Original combination (noise-driven position + twinkle) | idea-only | yes |
| Starfield ✦ | Sparse cool-white points with slow independent twinkle, night-sky feel | yes | Original combination | idea-only | yes |
| Confetti | Random points burst in saturated random hues then fade | yes | [FastLED: DemoReel100 (confetti)](https://github.com/FastLED/FastLED/blob/master/examples/DemoReel100/DemoReel100.ino) | code-ok (MIT) | yes |
| Popcorn | Points "pop" upward/outward from a base line then fall back | yes | [WLED: Popcorn](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Candle flicker | A single point (or few) flickers unevenly like a candle flame | 1D and 2D | [WLED: Candle](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Candle multi | Every point flickers independently, not in lockstep | yes | [WLED: Candle Multi](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Fireworks | Random blobs burst outward from a point then fade | yes | [WLED: Fireworks](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Bouncing balls (visual only) | Several points arc and bounce as if under gravity | yes | [WLED: Bouncing Balls](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Pixels | Isolated random pixels light briefly, sparse and irregular | yes | [WLED: Pixels](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Spots | Evenly spaced solid points of light along the piece | 1D and 2D | [WLED: Spots](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Spots fade | Evenly spaced points that pulse larger and smaller | yes | [WLED: Spots Fade](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |

## Whole-piece — everything together

| Name | What it looks like | 2D? | Source | Licence | Wearable? |
|---|---|---|---|---|---|
| Breathe / pulse | The whole piece fades smoothly in and out together (exists: `catalog/patterns/pulse`) | yes | [WLED: Breathe](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Blink | The whole piece alternates hard between two colours | yes | [WLED: Blink](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Fade | A smooth crossfade between two colours, whole piece | yes | [WLED: Fade](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| Strobe | The whole piece flashes in sharp bursts | yes | [WLED: Strobe](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | no (flash risk) |
| Heartbeat | A double-pulse rhythm like a heartbeat, whole piece brightness | yes | [WLED: Heartbeat](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| BPM pulse | Pulses of brightness timed to a tempo, moving gently back and forth | yes | [FastLED: DemoReel100 (bpm)](https://github.com/FastLED/FastLED/blob/master/examples/DemoReel100/DemoReel100.ino) | code-ok (MIT) | yes |
| Palette solid | The whole piece shows one palette colour, cycling slowly through it | yes | [WLED: Palette](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |

## Sims (optional) — state that evolves

| Name | What it looks like | 2D? | Source | Licence | Wearable? |
|---|---|---|---|---|---|
| Fire 2012 | Classic heat-diffusion fire simulation, rising and cooling (exists: `catalog/patterns/fire2012`) | 1D and 2D | [FastLED: Fire2012](https://fastled.io/docs/examples.html) | code-ok (MIT) | maybe (heat colours) |
| Fire 2023 | Updated fire simulation with finer heat diffusion | yes | [FastLED: Fire2023](https://fastled.io/docs/examples.html) | code-ok (MIT) | maybe |
| Pacifica (full sim) | Layered scrolling ocean waves with foam highlights, as an evolving state, not a static field | yes | [FastLED: Pacifica](https://fastled.io/docs/examples.html) | code-ok (MIT) | yes |
| Game of Life | Conway's cellular automaton scrolling/evolving across the grid | yes | [WLED: Game Of Life](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| PS Fire (particle) | Fire built from rising/cooling particles rather than a heat grid | yes | [WLED: PS Fire](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| PS Waterfall | Particles flow and pool like falling water | yes | [WLED: PS Waterfall](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | yes |
| PS Vortex | Particles swirl inward around a moving centre | yes | [WLED: PS Vortex](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| PS Ballpit | Balls fall and bounce off each other and the floor | yes | [WLED: PS Ballpit](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| PS Attractor | Particles orbit and get pulled toward a moving point | yes | [WLED: PS Attractor](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Black Hole | Points orbit and spiral inward toward a bright centre | yes | [WLED: Black Hole](https://kno.wled.ge/features/effects/) | idea-only (EUPL) | maybe |
| Boids / flocking ✦ | Points that flock together, steering as a group, classic Craig Reynolds boids | yes | Original (well-known algorithm, not tied to any one implementation) | idea-only | maybe |
| Langton's ant / turmite ✦ | A single moving state leaves a slowly building trail per simple local rules | needs structure (grid) | Original (well-known algorithm) | idea-only | no |
| Reaction-diffusion ✦ | Two virtual chemicals diffuse and react, producing organic evolving spots/stripes | yes | [Book of Shaders context / Gray-Scott model, general technique](https://thebookofshaders.com/) | idea-only | maybe (slow to read) |
| Meteor shower (exists) | Several meteors cross with trailing decay, staggered in time (exists: `catalog/patterns/meteor`) | 1D and 2D | in-house | n/a | yes |

## Needs structure (out of scope for the generic-plane library, listed for reference)

| Name | What it looks like | Why it needs structure | Source | Licence |
|---|---|---|---|---|
| Scrolling text | Readable text characters scroll across the piece | needs a pixel-font grid with fixed resolution | [WLED: Scrolling Text](https://kno.wled.ge/features/effects/) | idea-only (EUPL) |
| Tartan / plaid | Woven-looking bands crossing in a regular grid | needs a regular rectangular grid to read as woven | [WLED: Tartan](https://kno.wled.ge/features/effects/) | idea-only (EUPL) |
| GEQ / audio equalizer bars | Vertical bars whose height reads as frequency energy | needs a fixed column layout (and audio input, out of scope here too) | [WLED: GEQ](https://kno.wled.ge/features/effects/) | idea-only (EUPL) |
| Traffic light | Recognisable red/amber/green traffic-signal sequence | needs three fixed, named regions | [WLED: Traffic Light](https://kno.wled.ge/features/effects/) | idea-only (EUPL) |
| PacMan | A small maze-chase game rendered on the grid | needs a fixed maze layout and game state | [WLED: PacMan](https://kno.wled.ge/features/effects/) | idea-only (EUPL) |
| DNA double helix | Two strands twisting around a straight central axis with visible rungs | reads correctly only along a fixed straight/path axis | [WLED: DNA](https://kno.wled.ge/features/effects/) | idea-only (EUPL) |
| PS Spring / Pinball | Particles connected by springs, or bouncing between fixed walls | needs fixed anchor points or wall geometry | [WLED: PS Spring](https://kno.wled.ge/features/effects/), [WLED: PS Pinball](https://kno.wled.ge/features/effects/) | idea-only (EUPL) |
| Akemi | The WLED mascot's face reacts and dances | a fixed named-pixel mascot face | [WLED: Akemi](https://kno.wled.ge/features/effects/) | idea-only (EUPL) |
| Sample-geometry patterns (per-letter, along-stroke, wire order) | Motion that follows the physical wiring path of a piece (e.g. the choker's pen-stroke letter order) | needs sample (per-lamp) geometry — parked per `vision.md` D2/Future work | in-house (`vision.md`) | n/a |

---

## Sources consulted

- [WLED: List of effects and palettes](https://kno.wled.ge/features/effects/) — EUPL, idea-only
- [FastLED example sketches](https://github.com/FastLED/FastLED/tree/master/examples) — MIT, code-ok
- [FastLED: DemoReel100.ino](https://github.com/FastLED/FastLED/blob/master/examples/DemoReel100/DemoReel100.ino) — MIT, code-ok
- [Pixelblaze community pattern library](https://patterns.electromage.com/) and [zranger1/PixelblazePatterns](https://github.com/zranger1/PixelblazePatterns) — mixed/unclear per-pattern licensing on the forum and library; treated idea-only throughout
- [Resolume / Wire / FFGL & ISF plugin ecosystem](https://www.resolume.com/blog/11828) — proprietary + mixed third-party plugins, idea-only
- [MADRIX 5 effects](https://help.madrix.com/m5/html/madrix/hidd_handling_selection_effects_.html) — proprietary, no public per-effect list found beyond category names; not able to source individual entries, see deviation note below
- [Chromatik / LX Studio](https://github.com/heronarts/Chromatik) and [Titanic's End LXStudio-TE](https://github.com/titanicsend/LXStudio-TE) — non-commercial-capped licence (not open source); idea-only; the TE pattern-directory fetch 404'd (path guessed wrong) so no LX-specific entries were sourced this sweep
- [The Book of Shaders](https://thebookofshaders.com/) (noise, fBm, cellular/Voronoi, shapes chapters) — idea-only, technique reference
- [Inigo Quilez: Domain Warping](https://iquilezles.org/articles/warp/) and [2D distance functions](https://iquilezles.org/articles/distfunctions2d/) — idea-only, technique reference
- [Shadertoy](https://www.shadertoy.com/) kaleidoscope/domain-warp search results — CC BY-NC-SA 3.0 default per-shader licence, idea-only
- [Adafruit NeoPixel Überguide](https://learn.adafruit.com/adafruit-neopixel-uberguide) and [CircuitPython LED Animations](https://learn.adafruit.com/circuitpython-led-animations/overview) — code MIT (per Adafruit's own contribution docs), idea-only for descriptions here since no code is reused

### What didn't pan out

- **Madrix and Resolume** have no public, stable per-effect name list the
  way WLED does (Madrix's effects live behind product docs and the
  software itself; Resolume's "effects" are largely third-party FFGL/ISF
  plugins with no canonical list). Their entries above are folded into
  other families' descriptions (e.g. kaleidoscope, colour wipes) rather
  than listed as a separate Madrix/Resolume section — there wasn't a
  sourceable list to sweep.
- The **Titanic's End (`LXStudio-TE`) pattern directory** fetch returned a
  404 — the guessed path was wrong and a repository browse wasn't
  attempted further within this sweep's scope. Chromatik/LX ideas above
  come from general knowledge of the ecosystem (audio-reactive, particle,
  and geometric patterns) rather than a swept file list; flagged as a
  thinner source than the others.
- **Burning Man / art-car culture** didn't turn up a single sweepable
  source list (unlike WLED/FastLED's structured effect/example lists) —
  its ideas are already well represented via Pixelblaze (the dominant
  Burning Man LED controller) and the Points/Sims families above.
