# lpfx architecture

**Status:** Living draft. Captures the architectural model agreed during
the post-fluid-demo design discussion (April 2026). Multiple rounds of
stress-testing collapsed premature distinctions, surfaced the rig +
outputs concern, established arity as the primary classification of
texture-producers, named the four visual kinds (Pattern / Effect /
Mixer / Show), and locked in vocabulary. Will be revised as the example
pass and inputs/modulators design surface new requirements.

**Scope:** The structural shape of lpfx — what the layers are, what each
one owns, how they compose. **Not** a schema spec. Field names are
illustrative; concrete TOML schemas come later, after the example pass
forces us to nail them down.

**Source material:**
- `docs/story/2026-04-20-thesis-and-validation.md` — narrative,
  positioning, prior-art analysis. The "why."
- `docs/design/lpfx/examples/` (TBD) — concrete visual / layout
  descriptions that this architecture must support. The "what for."

---

## Vocabulary

### Visual is the umbrella

A **visual** is the universal noun of lpfx: any standalone artifact
that produces a 2D texture. There are four kinds, distinguished by
two structural axes — **how many primary input textures they take**
(arity) and **whether they have temporal state** (over-time vs.
at-once).

|  | At-once (stateless w.r.t. inputs) | Over-time (may be stateful) |
|---|---|---|
| **0 inputs** | **Pattern** — generates content | (a Pattern with internal state, e.g. fluid sim) |
| **1 input** | **Effect** — transforms one input | (an Effect with internal state, e.g. feedback trail) |
| **N inputs** | **Mixer** — combines N inputs simultaneously | **Show** — selects 1-of-N over time, with transitions |

The four kinds:

- **Pattern** (0-arity) — generates content from parameters, internal
  state, and modulators. The thing that's actually playing on screen.
- **Effect** (1-arity) — transforms a pattern. Audio-pedal mental model.
- **Mixer** (N-arity, ≥2) — combines multiple textures into one,
  stateless w.r.t. its primary inputs. The building block for blends,
  crossfades, picture-in-picture.
- **Show** (N-arity, ≥1) — selects from a set of patterns over time.
  Stateful, time-aware. The unit of "what plays when."

Mixer and Show are duals. Both N-arity, both produce one texture from
many candidates. The difference is **simultaneous combine (mixer)** vs.
**select over time (show)**.

### Why "visual" and not something more abstract

We considered "module" but rejected it. **Visual is specific on
purpose.** If we ever build audio-producing modules (synthesis, beat
generation), they'd be a different kind of thing — they don't produce
textures, they produce samples — and they deserve their own honest
name (`Sound`, `Sonic`, etc.), not a retconned "module" bucket.
Specificity is the design move; the over-abstract umbrella is the
dodge.

"Visual" is also the established term in the VJ / lighting community —
"visuals software," "I run visuals" — which is exactly the audience
we're courting.

### Code- and file-level naming

Internally:
- The crate is `lpfx`.
- Types are unprefixed inside the crate: `lpfx::Pattern`,
  `lpfx::Effect`, `lpfx::Mixer`, `lpfx::Show`, `lpfx::Visual` (the
  enum / trait covering all four).
- File extensions on disk: `*.pattern.toml`, `*.effect.toml`,
  `*.mixer.toml`, `*.show.toml`. The kind is visible from the
  filename.

When discussing all four together in writing, say "visuals" or
"patterns, effects, mixers, and shows." When discussing one
specifically, name it.

---

## The four layers (and one peer artifact)

| Layer | What it is | Portable across? | Persona |
|---|---|---|---|
| **Visual** | Standalone artifact that produces a 2D texture (Pattern, Effect, Mixer, or Show). All four are recursive (can reference other visuals). | Anywhere with the right runtime + referenced visuals | Coders + AI for leaf visuals; artists for composed visuals |
| **Layout** | Standalone artifact. A 2D pixel canvas with fixtures placed on it. Each fixture carries its visual placement, LED specs, **and its wiring** (output ref + universe + offset). May declare a default show. | Shareable (copy + re-point output refs); not abstractly portable | Owner / installer of the hardware |
| **Rig** | Standalone artifact. References layouts and outputs. Adds optional rig-level concerns (global dimmer, future power budget). | Yes (across projects) | Owner / installer of the hardware |
| **Project** | Top-level deployment artifact. References a rig and shows. Assigns shows to layouts. Holds project-level glue. | **Not portable** — a project is a specific deployment | Owner of the device; the unit of install |

Plus one peer artifact:

| Peer | What it is | Portable across? |
|---|---|---|
| **Output** | Standalone artifact. Hardware delivery config (protocol, address, optional constraints, optional max-current cap). | Yes (across rigs) |

The pattern: **content (visuals) → physical surface (layout) →
hardware infrastructure (rig + outputs) → deployment (project).** Most
artifacts are sharable. Only the project is deployment-specific by
construction.

There are **eight file types**: `*.pattern.toml`, `*.effect.toml`,
`*.mixer.toml`, `*.show.toml`, `*.layout.toml`, `*.output.toml`,
`*.rig.toml`, `*.project.toml`. One concept per file. Visual kind is
visible from the filename.

---

## Layer 1: Visuals

A visual is a recursive, self-contained artifact that produces a 2D
texture. The four kinds (Pattern, Effect, Mixer, Show) share most
structure and diverge only in arity and whether they're stateful /
time-aware.

### Output: always a 2D texture

Every visual, regardless of complexity, produces a single 2D texture.
There are no separate output modalities. A "1D" visual renders at
`Nx1`. A matrix visual renders at `NxM`. A "dome" visual renders at
whatever 2D resolution the layout asks for. The engine treats them
uniformly. The texture is the universal output.

Visuals may declare a **preferred resolution** (or "any") to inform
layout-vs-visual resolution negotiation, but the output type is fixed.

### Arity and the at-once / over-time axis

The four kinds sort along two axes:

- **Arity (primary inputs):** 0 (Pattern), 1 (Effect), N≥2 (Mixer and
  Show).
- **Combination semantics:** "at once" means the visual produces its
  output from current input frames + parameters, no internal selection
  over time. "Over time" means the visual picks among its inputs over
  time and may have selector state.

This taxonomy is exhaustive for what we want to support today. N-arity
selection was a missing concept until Show was promoted to a visual
kind; once it's there, the model is complete.

#### What can be scheduled where

- **Patterns** can stand alone as the active visual on a layout. They
  are what shows schedule.
- **Effects** cannot stand alone — they need an input upstream
  (typically a pattern). They appear in module pipelines.
- **Mixers** cannot stand alone — they need N inputs upstream. They
  appear in pipelines and inside transitions.
- **Shows** stand alone like patterns (they produce a texture and have
  no required external input), and importantly can themselves be
  referenced by other visuals — including being scheduled inside
  another show. Show-recursion is real and useful (see "Show
  recursion" below).

#### Composed visuals: arity is the net arity of the pipeline

A composed visual's kind is determined by the net arity of its
pipeline:

- `[pattern, effect, effect]` → net 0-arity → a Pattern.
- `[effect, effect]` → net 1-arity → an Effect.
- `[show1, show2, mixer]` → net 0-arity (sources are stand-alone) →
  a Pattern (yes, even though shows are inside!).

Validated by the engine at compose time. The file extension must
match the net arity.

#### Config inputs vs. primary inputs

Visuals can declare typed parameters that include **textures** as
their value type — palette LUTs, masks, displacement / flow fields,
reference images, even "render this other pattern into a buffer and
use it as my displacement map."

These are **config inputs, not primary inputs.** They don't change the
visual's kind. A Pattern that takes a palette-texture config parameter
is still a Pattern. An Effect that takes a mask-texture config
parameter is still an Effect.

The distinction: primary input is "what am I transforming or
selecting"; config input is "how am I configured." Same texture type
at the engine level, different role in the mental model.

### Recursion (the unification of leaf and composed visuals)

There is no separate "composition" type. A visual is a TOML artifact
with a list of pipeline steps. A step can be:

- A **shader** step (GLSL fragment over the visual's working canvas).
- A **compute** step (GLSL compute, where supported).
- A **builtin** step (Rust-native builtin like `msafluid`, `cellular`,
  `fire2012`).
- A **visual** step (a reference to another visual of any kind, with
  parameter bindings).

A "leaf visual" is just a visual whose steps are shader / compute /
builtin. A "composed visual" is just a visual that mostly contains
`visual`-kind steps wiring other visuals together. Both load through
the same code path, share the same UI, ship in the same format.

This mirrors the **Effect Rack / Instrument Rack pattern** from Ableton
Live: a rack is itself a plugin, but its contents are other plugins.
From outside, a rack IS a plugin. Same shape works here.

### What every visual declares

- **Identity** — name, version, author, license.
- **Kind** — pattern / effect / mixer / show (also implicit in the
  file extension).
- **Preferred resolution** — `NxM`, `any`, or a hint like "match
  fixture density."
- **Parameters** — typed, named, with defaults and ranges. Become UI
  knobs in any host that reads the visual.
- **Pipeline** — the ordered list of steps.
- **Internal state** — declarative description of any persistent
  buffers between frames (for stateful visuals).

### What a visual does NOT know

- Which layout(s) it'll render to.
- How many physical lamps exist anywhere downstream.
- Its host (CLI, ESP32, wgpu desktop, web preview).

These exclusions are deliberate. They keep visuals portable and
testable in isolation.

### Coder vs. artist visuals

Both produce the same artifact format. They differ only in step
composition:

- **Coder visual:** primarily shader / compute / builtin steps.
- **Artist visual:** primarily `visual` references with modulator
  bindings and re-exposed parameters.

A non-coder can author an "artist visual" by combining coder-shipped
visuals in the host UI. Same TOML on disk; same install path.

---

## Layer 1a: Pattern

The default visual kind. 0-arity, may be stateless or stateful.

Examples:
- Stateless: rainbow, plasma, palette gradient, noise field.
- Stateful: fluid (with internal emitters), fire2012, cellular
  automata, sand-physics.

Patterns are what shows schedule. Composed visuals that net to 0-arity
also count as patterns and can be scheduled.

Nothing structurally special beyond what's in the Visual section
above.

---

## Layer 1b: Effect

A 1-arity visual: takes one input texture, produces a transformed
output. The audio-pedal mental model.

Examples:
- Stateless: kaleidoscope, hue rotate, palette map, mirror,
  edge detect, blur (single-pass).
- Stateful: feedback trail (multi-pass + persistent buffer).

Effects appear inside the pipelines of other visuals. They cannot
stand alone in a show — that's enforced at compose time by the file
extension and the pipeline-arity check.

---

## Layer 1c: Mixer

An N-arity (N≥2) visual: combines multiple input textures into one
output. Stateless with respect to its primary inputs (a mixer's output
this frame depends only on its inputs this frame and its current
parameter values).

Examples:
- Crossfade — 2-input weighted blend driven by a `mix` parameter.
- Blend mode — 2-input multiply / screen / overlay / etc.
- Picture-in-picture — 2-input spatial composite (one inset into the
  other at a configurable rect).
- N-channel mixer — N inputs with per-input level controls (for
  audio-driven pattern stacking).

Mixers are the **building block of transitions** at the show layer
(see Show below). They are also useful as standalone composition
primitives — e.g., "permanently multiply this audio-fluid pattern by
this mask pattern" is a Mixer baked into a composed Pattern.

Mixers can have internal state in principle (same machinery as
stateful effects), but in practice this is rare. The defining property
is N-arity primary inputs combined "at once," not statefulness.

---

## Layer 1d: Show

An N-arity, stateful, time-aware visual. A show **selects** one (or
sometimes a transitioning blend of two) of its candidate patterns to
be active over time, and outputs that one's texture. State and time
are first-class for shows: they own selection state, transition state,
modulator routing, and the temporal logic that says "what plays when?"

This is the layer lp2014 partially had (live-show priority selection)
and lp2014 entirely lacked (timelines, real cue lists). Shows are
where lpfx's "production-grade" claim lives.

### What a show owns

- **A library of candidate patterns** — visuals (any kind whose net
  arity is 0) that this show might play.
- **A selector** — the rule that picks which pattern is active. The
  shape of the selector depends on show type (see below).
- **Transitions** — how the active pattern changes when the selector
  picks a new one. Transitions are typically implemented as a 2-input
  Mixer (crossfade / wipe / cut) with the mix parameter automated.
- **Modulator routing** — global LFOs, audio bands, beat detection,
  MIDI/OSC inputs, IMU streams, time/clock. Bound to pattern
  parameters and (often) to the selector itself. Designed separately
  (deferred), but show is the layer where modulator bindings live.
- **Fallback rules** — what plays when nothing else qualifies. Lp2014's
  gap. In a live show, fallback is just another candidate pattern with
  low priority.

### What a show does NOT know

- Which layout(s) it'll be rendered onto.
- The resolution(s) it'll be rendered at.
- The number of lamps downstream.

### Three show types

There are three show types, distinguished by their **selector shape**
and their **loading model**. Loading model matters operationally:
patterns can be expensive (a fluid sim takes seconds to warm up and
hundreds of KB of buffer state), so how many are kept alive
simultaneously is an architectural property, not an implementation
detail.

| Type | Loading model | Selector | Transitions | Use case |
|---|---|---|---|---|
| **Live** | All candidates loaded simultaneously, all running internally | Self-reported priority + manual cue | Default policy + per-pair overrides | Festival / installation / club; instant-react needed |
| **Playlist** | Bounded (current + next pre-rolling); inactive candidates cold | Manual cue, sequence, or schedule | Default policy + per-pair overrides | DJ-style ambient sets; theatrical cue lists |
| **Timeline** | Bounded (time-windowed pre-roll); inactive candidates cold | Wall clock | Per-pair authored, baked in place | Choreographed pieces synced to fixed media |

#### Live shows

All candidate patterns are loaded and running internally. They are not
all *active* (only one is sampled and output at a time, modulo
transitions), but they're all alive — accumulating internal state,
warming up fluid sims, tracking audio, etc.

Selection is **self-reported priority**, the lp2014 pattern: each
candidate visual reports a priority value at selection time. The
selector picks the highest-priority candidate (with stable
tiebreaking via a configured ordering). Examples:

- An audio-reactive fluid pattern reports `HIGH` priority when audio
  energy exceeds a threshold, `NONE` otherwise.
- A camera-fed pattern reports `HIGH` when a video signal is present.
- A demo loop pattern reports `LOW` priority always — it's the silent-
  fallback.

When inputs change, the selector switches and a transition fires.
Manual cues override the priority sort (operator force-selects a
specific candidate).

This pattern is brilliant because **candidates own their own
activation rules.** Adding a new candidate to a live show doesn't
require updating any selector logic. The candidate brings its
"when am I relevant?" with it.

The cost: all candidates running simultaneously means the total
resource footprint (memory, CPU, GPU) is the *sum* of all candidates,
not the *max*. On constrained hardware (ESP32-class), the candidate
count for live shows is small (2–4). On desktop / P4, much larger.
This trade-off is what makes the loading model worth distinguishing.

#### Playlist shows

A bounded set of candidates is loaded at any time — typically the
current one plus the next one pre-rolling for a clean transition.
Cycling through candidates is sequence-driven (next button, scheduled
time, etc.) rather than data-driven.

Resource footprint is bounded (≤2 visuals worth of state typically),
which makes playlist shows the right answer when you have many
candidates but limited hardware. The trade-off is loss of instant-
react: switching from candidate A to candidate Z requires loading Z,
which has cost.

#### Timeline shows

A timeline is an ordered sequence of `(pattern, transition)` pairs,
with absolute wall-clock anchoring. Each transition is **authored in
place** (this radial wipe, synced to that beat marker) rather than
derived from a default policy. This is the "tape" model — what plays
is a function of time only.

Loading is windowed: the timeline pre-rolls upcoming patterns shortly
before they're needed. Resource footprint is bounded similarly to
playlist shows.

### Show recursion

Shows are visuals. Therefore shows can reference other shows the same
way patterns can reference other patterns. The most common case:
**fallback via composed shows.**

```
my-festival-show.show.toml  (live)
  candidates:
    - audio-fluid                        priority: when audio detected
    - camera-feed                        priority: when video detected
    - silence-fallback.show.toml         priority: always low (a timeline
                                          of curated demo patterns)
```

When audio and video both go quiet, the live show selects the timeline
fallback — which is itself a show, scheduling its own patterns over
time. Recursion eliminates the need for special "fallback" infrastructure;
fallback is just another candidate pattern that happens to be a show.

Same recursion enables layered shows via Mixer-over-Show: a Mixer
with two Show inputs is a Pattern that blends two ongoing shows
permanently. Useful for "ambient base layer + audio-reactive overlay"
arrangements.

### Transitions vs. mixers

These are two distinct concepts that work together:

- A **mixer** is a stateless N-input → 1-output texture combiner.
- A **transition** is a show-level temporal primitive: "from active
  pattern A, become active pattern B, over T seconds, easing E."

A transition is **implemented by** a mixer (typically a 2-input
crossfade) with the mix parameter automated by the show's transition
state. The catalog of "transition types" is roughly the catalog of
2-input mixers crossed with the catalog of timing curves.

Keeping these separate means new transition styles are just new
mixers — composable, shareable, no special infrastructure.

### Snapshots and tween-cues (orthogonal, future)

A separate concept worth noting: **parameter snapshots** (lp2014's
`TweenManager`). A snapshot is a captured state of all named
parameters across all candidate patterns at one moment. Tweening
between snapshots smoothly morphs parameters without changing the
active pattern.

This is orthogonal to pattern selection and transitions. It's the
"cue list of looks" feature found in lighting consoles — you snapshot
twenty tunings of the same patterns and morph between them on cues,
while pattern selection is doing its own thing.

Designed separately, deferred to future work. Lp2014's TweenManager is
the reference.

### Why "show"

- "Composition" is overloaded — and unified with visual.
- "Set" (DJ term) is too narrow — implies live performance only.
- "Show" covers timeline (theatrical), playlist (background ambient),
  live performance (festival), and installation (museum) cases.
- Lp2014 used "show" for the equivalent concept; most lighting and
  theatrical software uses it.

---

## Layer 2: Layout

**A layout is a 2D pixel canvas with fixtures placed on it.** It is
where visual structure lives. Critically, fixtures carry their
**wiring** inline (which output, universe, channel offset) — see
"Outputs and wiring" below for the rationale.

### One layout type, not many

The engine treats all layouts identically: a 2D pixel grid with
fixtures placed at coordinates. A 100-LED strip is `100x1`. A 16×16
matrix is `16x16`. A dome of 190 panels is some explicit canvas size
(e.g. `128x128`) with each panel mapped to a polygon region via map
projection.

UX may show different editing surfaces for different shapes (strip
ribbon editor vs. matrix grid vs. free-form canvas drag), but the
data model and rendering code are uniform.

3D fixtures (domes, orbs, tree-of-tenere-style shapes) are handled
by **map projection to 2D** in the layout file. Real 3D rendering is
explicitly out of v1.

### What a layout owns

- **Resolution** — the pixel dimensions of the canvas. Either:
  - **Explicit** (e.g. `128x128` for the dome).
  - **Auto-derived from fixtures** (e.g. one 100-LED strip → `100x1`).
- **Fixtures** — one or more. Each fixture declares:
  - **Lamp positions** — coordinates within the canvas (visual).
  - **LED specs** — color format (RGB / RGBW), color order
    (RGB / GRB / etc.), lamp count, optional max-current cap.
  - **Per-fixture calibration** — gamma curve, white balance,
    brightness limit.
  - **Wiring entries** — one or more `(output_ref, universe?,
    offset)` tuples. One fixture can have **multiple wiring entries**
    if it's split across outputs (lp2014's "sub-fixture" / "mapping
    element" pattern). Universe is used by output kinds that have it
    (Art-Net, sACN, OPC); ignored by RMT/SPI.
- **Optional default show** — a layout can declare "if no project
  overrides, use this show." Useful for layouts that ship as
  ready-to-use packs.

### Sampling rule (the only one)

For each lamp at position `(x, y)` within `[0, W) × [0, H)`, sample
the show's currently-active pattern output at that position. Done.
Pixel-perfect behavior is **emergent**, not configured: if a layout's
lamp positions land on integer pixel centers (a 100-LED strip in a
100×1 layout, a 16×16 matrix in a 16×16 layout), sampling is
automatically pixel-perfect. If positions are sub-pixel (a polygon-
mapped dome), the runtime interpolates. No sampling-mode flag.

### Resolution authority

- Layout-natural-resolution wins by default.
- A visual's preferred resolution is a hint; the layout's resolution
  drives.
- Project may override layout resolution (e.g. "render at 32×32 even
  though the layout suggests 128×128 — I'm on a slow MCU"). Per-project
  override only; doesn't mutate the layout file.

### Portability tradeoff

Because fixtures carry their output references inline, **a layout file
is shareable but not abstractly portable across rigs.** Moving a
layout to a different rig requires re-pointing the output references
(or running the import-time auto-rewire UX, which is future work).

This is the same tradeoff lp2014 made and lived with successfully. The
alternative — pulling wiring out into the rig as a separate
`fixture-id → output, universe, offset` table — restored portability
but added an indirection layer that hurt the common case. We're
optimizing for the common case.

### Why "layout"

- Lighting / display tradition: a "layout" of fixtures is a familiar
  noun.
- Short, concrete.
- Doesn't claim "rig" (which is the whole hardware setup, larger than
  one layout).

---

## Layer 3: Rig

**A rig is the layer that aggregates layouts and outputs into a
coherent hardware setup.** It is the answer to "what physical
deployment exists, considered as a whole?"

### What a rig owns

- **Layout references** — which layouts compose this rig.
- **Output references** — which outputs are available in this rig.
- **Optional rig-level dimmer** — global brightness multiplier for
  everything driven by this rig.
- **Future: power-budget object** — a rig-level power supply / current
  budget that fixtures and outputs contribute to. **Not in v1**; v1
  uses optional per-fixture and per-output current caps only. See
  "Power budgeting" below.

### What a rig does NOT own

- Wiring details. Those live on the fixtures inside the layouts.
- Show content. That's the project's job.
- Visual layout placement. That's the layout's job.

### Why "rig"

- Lighting and live-performance tradition: "rig" = the whole physical
  setup of fixtures + cabling + outputs.
- Distinct from "layout" (one visual surface) and "project" (a specific
  deployment of a rig + shows).
- Already in the working vocabulary of the user community we're
  serving.

---

## Layer 4: Project

**A project is the top-level deployment artifact.** It picks a rig,
picks the shows that will play on it, and assigns shows to layouts.

### What a project owns

- **Rig reference** — which rig this project deploys to.
- **Show references** — which shows ship with this project.
- **Assignments** — `(show, layout, mode)` tuples. A show can be
  assigned to one or more layouts. A layout can override its default
  show.
- **Calibration overrides** — per-project tweaks on top of layout /
  fixture defaults (rare; mostly a per-project brightness limit).
- **Project-level glue** — audio source device, MIDI routing,
  persistence location, default-on-boot show, etc.
- **Save state** — last-played pattern, current parameter values, etc.

### Multi-layout assignments

A show can drive multiple layouts at once. Real example: dome main
surface + a mirrored orb on a stage nearby, both running the same
audio-fluid show.

```toml
# project.toml (illustrative)
[[assignments]]
show = "audio-fluid"
layouts = ["dome-main", "stage-orb"]
mode = "mirror"   # or "duplicate"

[[assignments]]
show = "doors-loop"
layouts = ["dome-doors"]
```

Two render strategies, picked per assignment:

- **`mirror`** — render the show once, feed the result to all
  assigned layouts (each samples it at its own resolution). Preserves
  literal synchronization, including stochastic effects (sparkles
  match exactly). Cheaper.
- **`duplicate`** — render the show separately for each assigned
  layout, each at its native resolution. Each layout gets pixel-perfect
  content. Stochastic effects diverge. More expensive.

Default mode TBD; probably `mirror`.

### Minimum-viable projects

Projects don't have to use every feature:

```
Customer / shipped art:  project → rig (with one layout, default-show declared)
Artist / composition:    project → rig + one assigned show
Performer / install:     project → rig + multiple shows + multi-layout assignments
```

Same engine, same artifact format, three usage tiers — supported by
the project layer being permissive about what it points at.

---

## Cross-cutting: Outputs and wiring

This is the layer where physical hardware delivery happens. The
fixture/output decision boundary is the single most consequential
shape decision in the architecture, so it gets its own section.

### Output is its own artifact

An output describes one delivery channel — a GPIO pin running WS2811,
an Art-Net node, an OPC server, a single-LED PWM, etc. It's its own
file (`*.output.toml`) so output configurations can be reused across
rigs (you have one Pixlite at one IP; multiple rigs may want to
target it). In practice they're often inline-by-convention to a single
rig directory; promotion to a shared file is straightforward when
needed.

### What outputs declare vs. what fixtures declare

| Concern | Lives where | Why |
|---|---|---|
| Lamp count, positions in canvas | **Fixture** | Visual / physical truth of the LED hardware |
| Color format (RGB vs RGBW) | **Fixture** | The LED chip itself is 3 or 4 channels |
| Color order (RGB vs GRB vs BGR) | **Fixture** | Manufacturer choice baked into the chip |
| Per-lamp current / voltage limits | **Fixture** | Hardware property of the LED |
| Per-fixture calibration (gamma, white balance) | **Fixture** | Tuning the physical fixture |
| Protocol (WS2811, sACN, Art-Net, OPC, …) | **Output** | The delivery channel determines the wire format |
| Local vs networked | **Output** | GPIO pin vs IP address |
| Bandwidth limits (universe size, packet rate) | **Output** | Protocol-determined constraint |
| Output-enforced constraints (e.g. "Pixlite wants all RGB") | **Output** (declared as constraint) | Output-side decision that propagates to fixtures via validation |
| Wiring (which output a fixture goes to + universe + offset) | **Fixture** (one or more entries) | Lp2014 precedent; matches installer mental model |
| Output-level max-current cap (optional) | **Output** | Power supply ceiling on a delivery channel |

### Outputs are dumb channel lists

Most of the time, outputs are stateless lists of channels — they
accept "set channel N to value V" and deliver it via their protocol.
Fixtures own format/order/calibration; outputs are the dumb pipe.

For introspection (debugging UI, "what's on this output"), an output
can answer by walking the fixtures that wire to it. No need for the
output to duplicate that data.

### Wiring lives on the fixture

Each fixture declares one or more wiring entries:

```toml
# layout.toml (illustrative — fixture inside a layout)
[[fixtures]]
id = "panel-001"
lamps = [...]
color_format = "rgb"
color_order = "grb"
lamp_count = 144
calibration = { gamma = 2.2 }
wiring = [
  { output = "pixlite-1", universe = 1, offset = 0 }
]

[[fixtures]]
id = "split-panel-007"
lamps = [...]
color_format = "rgb"
color_order = "grb"
lamp_count = 288
wiring = [
  { output = "pixlite-1", universe = 7, offset = 0 },
  { output = "pixlite-1", universe = 8, offset = 0 },  # second half
]
```

### One output → many fixtures

The typical case is many fixtures sharing one output (190 dome panels
on one Pixlite, 2 strips on one RMT pin). The wiring-on-fixture model
handles this naturally — multiple fixtures reference the same
`output_id` with different universes / offsets.

### Output constraints propagate at compose time

If an output declares `constraints = { color_format = "rgb" }`
(a Pixlite that wants only 3-channel data), the rig compile walks all
fixtures wired to that output and validates they match. Mismatch =
error before runtime.

### Auto-wiring is UX, not engine

For the dome's 190 panels, you don't write 190 wiring lines by hand —
the UX handles "wire panels 1..190 sequentially across universes
1..190" via a generator. The engine consumes only the **expanded**
wiring; the UX layer is responsible for any sugar.

This boundary keeps the engine simple. Auto-wiring is a known major
domain (lp2014 + Chromatik both have rich auto-mapping UX) but it
sits firmly above the engine.

---

## Cross-cutting: Power budgeting

**v1: optional per-fixture and per-output max-current caps.** Both can
declare a cap; the runtime enforces by clamping aggregate brightness.
Most installs ignore this entirely.

**Future: a first-class power-budget object** in the rig that
fixtures contribute to. This is the right model for serious installs
where multiple fixtures share a power supply, or where one fixture
spans multiple supplies. Captured as future work; not in v1.

Battery-powered devices (wearables, small art) benefit most from the
v1 cap-based model. Big mains-powered installs typically have
oversized supplies and don't need it.

---

## Cross-cutting: Calibration

- **Per-fixture (default)** — gamma curve, white balance, brightness
  limit. The fixture is where most calibration lives, because the
  variation that needs correcting is per-fixture (different chip
  batches, different mounting, different physical conditions).
- **Rig-level global dimmer** — one number that scales everything
  driven by the rig. The most common rig-wide tweak.
- **Project-level overrides** — per-project tweaks on rig defaults.
  Rare; mostly used for "performance mode" vs "ambient mode" brightness
  differences in the same hardware.

That's it. No deeper hierarchy needed.

---

## Cross-cutting: Validation responsibilities

| Layer | Validates |
|---|---|
| **Pattern / Effect / Mixer** | Pipeline steps reference valid step kinds; param types are well-formed; declared internal state is consistent; pipeline net arity matches the file kind (`*.pattern.toml` → 0, `*.effect.toml` → 1, `*.mixer.toml` → ≥2). |
| **Show** | Candidate references resolve and have net arity 0 (i.e. are patterns or shows, not bare effects/mixers); transitions reference valid 2-input mixers; selector parameters bind to real modulators (when modulators are designed); per-pair transition overrides reference real candidates. |
| **Layout** | Lamp positions fit within layout resolution; LED specs are well-formed; wiring entries are syntactically valid (full resolution against outputs happens at rig compile). |
| **Output** | Hardware config is internally consistent; declared constraints are well-formed. |
| **Rig** | Every wiring `output_ref` resolves to an output that exists; every fixture's wiring respects its target output's constraints; aggregate channel use fits within per-output bandwidth; per-output current caps not exceeded if declared. |
| **Project** | Every assignment `layout` reference is in the rig's layouts; every assignment `show` reference exists; render mode is valid for the layout count. |

All validation at compose time, never at runtime. Runtime trusts
that compose-time validation passed.

---

## Cross-cutting: Inputs and modulators

**Deferred to a separate design pass.** Holding the model open here
because there are separate ideas about how this should work, and
because the input infrastructure may itself need a peer artifact
(analogous to how Output is the peer of Layout / Rig — there may be
an "input bench" or similar peer for audio interfaces, MIDI ports,
OSC endpoints, IMU streams, etc.).

What we know we'll need eventually:

- Audio analysis (FFT bands, beat detection)
- MIDI control input
- OSC input
- IMU streams (for wearables / kinetic art)
- Time / wall clock / beat clock
- LFOs

How they slot into the layers — at project level, at show level, as a
peer artifact, or via some unified routing primitive — is the next
conversation. Live shows particularly need this: candidate
self-reported priority is *driven* by inputs, so without the input
story, live shows can't actually be authored end-to-end.

---

## Cross-cutting: Parameter system

Parameters are typed, named, and uniform across all layers. Same value
types whether you're declaring a visual's `viscosity`, a project's
`default_brightness`, or a layout's `gamma_curve`.

Preliminary type set: `float`, `int`, `bool`, `color`, `palette`,
`enum`, `vec2`, `vec3`, `pattern-ref`, `effect-ref`, `mixer-ref`,
`show-ref`, `visual-ref` (any kind), `layout-ref`, `output-ref`,
`texture` (for config-input textures).

Each param has: type, default, range/options, label, description, UI
hint. The UI hint set should be small and AI-friendly: `slider`,
`color-picker`, `palette-picker`, `xy-pad`, `dropdown`, `toggle`.

---

## Cross-cutting: Persistence format

- **TOML** for all human-authored layer files (`*.pattern.toml`,
  `*.effect.toml`, `*.mixer.toml`, `*.show.toml`, `*.layout.toml`,
  `*.output.toml`, `*.rig.toml`, `*.project.toml`) — agent-friendly,
  git-friendly, comment-friendly.
- **GLSL files** referenced by name from visual TOMLs (one shader per
  file; multi-pass visuals reference multiple files).
- **Binary blobs** (palette tables, mapping LUTs, custom builtin
  state) referenced by relative path.
- **Directory layout:** each layer is a directory containing one
  TOML + supporting files. `patterns/`, `effects/`, `mixers/`,
  `shows/`, `layouts/`, `outputs/`, `rigs/`, `projects/`.

Sharing a project = zipping its directory plus the directories of the
rig + layouts + outputs + shows + visuals it transitively references.
Like a DAW project file with embedded plugin presets.

---

## Deliberate non-goals (what this is NOT)

- **Not a node-based visual editor at the engine level.** Lp2014's
  original idea was a node graph in the host; lpfx pushes nodes into
  the visual layer (as the step pipeline) and keeps higher layers
  arrangement-shaped, not graph-shaped.
- **Not 3D in v1.** 3D fixtures are handled by map projection to 2D
  layouts. Real volumetric rendering is future work and probably never
  v1.
- **Not stateless at the show level.** Shows own time and selection
  state by design. (Per-visual state lives in visuals; inter-visual
  state is open.)
- **Not multi-host distributed.** v1 = one project = one runtime.
  Multi-rig synchronized installations are future work.
- **Not strongly typed visual output modalities.** Visuals produce 2D
  textures; the resolution varies, but the type does not.
- **Not abstractly portable layouts.** Layouts are shareable by copy +
  re-pointing output refs (or future auto-rewire UX); they are not
  designed to drop into any rig untouched.
- **Not auto-wiring at the engine level.** Auto-wiring is a major UX
  domain and lives above the engine. The engine consumes expanded
  wiring only.
- **Not a real power-budget system in v1.** Optional per-fixture and
  per-output current caps only; first-class budget object is future
  work.
- **Not non-visual production.** Lpfx is texture-out only. If we ever
  build audio synthesis or kinetic / motor control, those are
  different artifact kinds with their own taxonomy, not retrofitted
  into the visual hierarchy.

---

## Lessons baked in (from prior art)

| Source | Lesson | How it shows up |
|---|---|---|
| **ISF** | Shader + typed-input manifest is the right shape for a leaf visual | Visual TOML structure |
| **ISF** | Multi-pass + persistent buffers are required for trail / feedback effects | Pipeline step kinds, declared internal state |
| **Chromatik / LX** | Lighting wants to be authored like music (DAW model) | The 4-layer hierarchy |
| **Chromatik / LX** | Pattern (0-input) vs Effect (1-input) split | Adopted directly as `Pattern` / `Effect` vocabulary; promoted to four kinds with Mixer / Show |
| **Chromatik / LX** | Modulation + audio + MIDI as first-class | Cross-cutting modulators (deferred design) |
| **Chromatik / LX (negative)** | Fixtures-as-Java-code locks out non-developers; no built-in mapping | Fixtures-as-data in TOML; mapping is layout's primary job |
| **Pixelblaze** | ESP32-class hardware can be the runtime | Project as install-on-MCU unit |
| **Pixelblaze** | "Pattern" as the vocabulary for self-contained programs | Same vocabulary adopted for 0-arity visuals |
| **Pixelblaze (negative)** | Wrong language (JS) and execution (interpreted) caps the ceiling | GLSL + JIT (lpvm) |
| **VJ / lighting community** | "Visual" is the established noun for what these things produce | Adopted as the umbrella term over "module" |
| **DAW** | Plugin / preset / track / song / project file | Visual / pattern-with-bindings / show / rig / project (more layers because spatial dimension exists) |
| **DAW** | Plugin format makes the ecosystem | Open visual format, not a closed plugin SDK |
| **DAW** | Mixer is a first-class concept, separate from effects | Mixer is its own visual kind (N-arity stateless), distinct from Effect |
| **Ableton Live** | "Rack" pattern: containers that look like single plugins from outside | Recursive visuals (a composition IS just a visual) |
| **lp2014 (`LightingScene.kt`)** | Candidate self-reported `activationPriority` works in production | Live shows: candidates self-report priority; selector picks the strongest claim |
| **lp2014 (`TweenManager`)** | Parameter snapshots + tweening is a separate, valuable concern from pattern selection | Snapshots / tween-cues noted as future work, separate from show transitions |
| **lp2014** | Canvas-only sampling fails for pixel-perfect 1D | 1D layouts are just `Nx1` 2D layouts; pixel-perfect is emergent from positions |
| **lp2014** | Multi-show on multi-layout is real and was awkward via shared canvas | First-class multi-layout assignments in projects |
| **lp2014 (gap)** | No story for "what plays when audio stops" | Live shows have always-on low-priority fallback candidates; show recursion lets fallback be a curated timeline |
| **lp2014 (gap)** | No real timeline / cued show concept | Three first-class show types (Live, Playlist, Timeline) |
| **lp2014** | Wiring-on-fixture (output + universe + offset) works in production | Same model adopted; tradeoff is layout portability, accepted |
| **lp2014** | Fixtures with multiple "mapping elements" (sub-fixtures sharing properties) | One fixture can have multiple wiring entries |
| **WLED / FastLED** | Pixel-perfect direct-strip work is a real and beloved use case | Falls out of `Nx1` layouts at the right resolution |
| **DMX / Art-Net world** | Universe + channel offset is the universal address vocabulary | Same vocabulary used throughout; degrades gracefully for output kinds without universes |
| **Mixer / Show duality** (derived) | N-arity-stateless and N-arity-stateful-time-aware are the two natural "combine many" semantics | Mixer and Show as distinct sibling visual kinds |

---

## Migration from existing lp-core

lp-core today has its own concepts: project (with a different meaning),
shader nodes, texture nodes, and the engine pipeline. The 4-layer model
above is a re-conception, not a refactor.

Mapping (rough):

| lp-core today | lpfx model |
|---|---|
| Project (with everything in it) | Split: layout(s) + outputs + rig + show(s) + project |
| Shader node | Step inside a visual |
| Texture node | Step inside a visual (likely a builtin or compute step) |
| Fixture (in lp-engine) | Fixture inside a layout, with wiring inline |
| The engine pipeline | The visual runtime |

**Migration is a separate project**, scoped and planned only after this
architecture is locked. Current code keeps working; new lpfx artifacts
ship alongside.

---

## Open questions

1. **Default render mode** for multi-layout assignments — `mirror` or
   `duplicate`?
2. **Inter-visual state** — can visual A read visual B's output buffer?
   Default no. Pushed on by fluid-sim-with-emitters cases.
3. **Inputs / modulators** — entire shape, including possible peer
   artifact for input infrastructure. Held until separate conversation.
4. **ISF importer scope** — v0 deliverable or v1? Likely v0 for
   bootstrapping the gallery.
5. **Hardware tier system** — ESP32-C6 / ESP32-P4 / desktop wgpu have
   very different perf budgets. Visuals should declare a minimum tier;
   runtimes should refuse to load above their tier. Particularly
   relevant for Live shows, where total footprint = sum of candidates.
6. **Live show pre-roll budgets** — when a fluid-sim candidate joins
   a live show, how is its warm-up time managed? Is "ready / not
   ready" a candidate-self-reported state alongside priority?
7. **Selector stickiness / hysteresis** — a candidate that drops from
   `HIGH` to `NONE` for 100ms shouldn't trigger a transition.
   Hysteresis is a per-show or per-candidate concern; design TBD.
8. **Snapshot / tween-cue system** — lp2014's TweenManager equivalent.
   Future work; exact integration with show types TBD.
9. **Cross-show coordination** beyond shared modulators — show-to-show
   triggers, cue chaining. Probably post-v1.
10. **Layout import / auto-rewire UX** — how does it actually feel
    when you drop a layout file into a different rig? Future UX work;
    engine stays simple.
11. **First-class power budget object** — what does it look like as a
    rig-level concept? Future work; v1 uses caps.
12. **Mixer cardinality** — most mixers will be 2-input. Are there
    real 3+-input cases worth designing for in v1? Driven by example
    pass.

---

## Pointers

- Story / positioning / why now: `docs/story/2026-04-20-thesis-and-validation.md`
- Concrete visual / layout examples (TBD): `docs/design/lpfx/examples/`
- Future-doc on inputs / modulators (TBD): `docs/future/...`
- Future-doc on snapshot / tween-cue system (TBD): `docs/future/...`
- The fluid-sim spike that proved the platform: `lp-fw/fw-esp32/src/tests/fluid_demo/`
- Lp2014's live-show selector reference: `/Users/yona/dev/personal/lightPlayer/PlayerCore/src/main/java/com/lightatplay/lightplayer/scene/LightingScene.kt`
