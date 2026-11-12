# lpfx thesis & idea-validation notes

Conversational notes captured during the fluid-demo spike. Raw material
for a future announcement / blog post / pitch — not a polished narrative.
Pull from these when writing public-facing copy.

## The one-line thesis

**"Shadertoy for LEDs, on a $3 chip, with AI as the author."**

Or longer: GLSL won as the lingua franca of generative graphics 15+ years
ago. Until very recently, nobody could run it on a microcontroller. Now
we can — and AI fluency in GLSL means non-coders can author shaders too.

## The core insight (Yona's framing)

> "We figured out GLSL was the lingua franca of generative graphics years
> ago. Why hasn't anyone used them on a microcontroller?"

That's the question the project answers. The answer: nobody could, until
the last 1–2 years. Now they can.

## The convergence (why now, not earlier)

Three things had to land independently. They all did, recently:

1. **MCUs got fast and RAM-rich enough.** ESP32-S3 / ESP32-C6 / RP2350 at
   160–240 MHz with 256–512 KB SRAM is the inflection point. Pre-2022 the
   math just didn't work for non-trivial generative graphics on a
   microcontroller — you'd get rainbows and chases and that's it. Our
   fluid sim running at 20 Hz on a $3 ESP32-C6 is **the proof.**

2. **Rust on bare metal made complex pipelines authorable.** Embedded
   C/C++ would still be possible but would cost ~10× the engineering
   time. Fixed-point arithmetic + a custom shader VM (lpvm) on a 32-bit
   integer-only RISC-V part is the kind of project that is brutally
   tedious in C and tractable in Rust.

3. **LLMs made GLSL authoring democratic.** This is the underrated one.
   Before AI, "you must write GLSL" gated 95% of users out. With AI,
   "describe what you want → get a working shader" is a real workflow.
   The AI is fluent in GLSL precisely *because* the corpus is enormous
   (Shadertoy alone). Which is itself a function of GLSL being the lingua
   franca. The whole loop closes.

## The proof point: fluid sim on ESP32-C6

April 2026. RGB MSAFluid solver, 24×24 grid, 4 Jacobi iterations, 20 Hz
solver step, 95 Hz display update with temporal interpolation between
solver frames. Drives a 241-lamp circular ring fixture via WS2812B over
RMT. Looks legitimately good.

Numbers people will care about:
- Hardware: ESP32-C6 (RISC-V, 160 MHz, no FPU, no GPU, ~$3 BOM).
- Solver: pure `no_std` Rust, fixed-point Q32 throughout.
- Memory: fits comfortably in SRAM with room to spare.
- Real time: 5 Hz solver looks smooth at 95 Hz display thanks to a fixed
  temporal-interpolation bug in the display pipeline.

The takeaway from the spike: GLSL-style stateful effects (fluid, particles,
cellular automata) can run on this class of hardware. Not all of them, not
at desktop resolution, but enough to make real art.

## Why nobody else did it

Honest read on the field — there are three adjacent communities, and none
of them sat at our exact intersection:

- **MCU LED community** is historically Arduino + FastLED + WLED
  hobbyists. Effects are hand-rolled C++ in a fixed library (rainbow,
  chase, fade). Not a programmable shading model. Not sharable.
- **Generative graphics community** built for desktop GPUs because that's
  where the money was. Shadertoy, ISF, TouchDesigner, Notch, Unreal —
  all assume a real GPU.
- **Pro lighting / installation software community** (Chromatik / LX
  Studio, TouchDesigner-for-LEDs setups, Madrix, Resolume) targets
  desktop machines that drive dedicated LED controllers over the network
  (Art-Net / sACN / OPC). Powerful, but assumes you have a computer in
  the rig.
- The three communities barely touched.
- Economic incentive was small until LED installs got big AND cheap MCUs
  got fast. Both happened ~2022–2024.

Two notable players already operating in nearby space: **Pixelblaze**
(Ben Henke) and **Chromatik / LX Studio** (Mark Slee).

## On Pixelblaze (personal note from Yona)

Yona knows Ben — same circles, nearby cities. Years ago Yona asked Ben
if he wanted to open-source the engine and collaborate. Ben declined,
which is fine — it's his business. (Mention this with care in any
public copy; it's friendly history, not a competitive jab.)

**Where Pixelblaze lands and where lpfx differs:**

- Pixelblaze runs JS effects on ESP32. Closest existing thing in spirit.
- JS is not the right language: it's not the lingua franca of generative
  graphics, so you don't inherit Shadertoy's 15-year corpus or AI
  fluency. The effect library is small and isolated; it can't import
  the rest of the world.
- Pixelblaze's engine is interpreted. We tried interpreting our IR
  (LPIR) and got ~6 fps on a desktop where the JIT version gets 30 fps
  on an ESP32-C6. Interpretation is a hard ceiling for full-fledged 2D
  shaders.
- JIT was the technical unlock that made this viable. **JIT is hard.**
  Yona's quote: "I couldn't have done it (in a reasonable time) without
  AI help." That's a meta point worth making in any announcement —
  this project exists because AI changes what one person can build.

So the field had one player who got the platform right (Pixelblaze: ESP32
+ effect library + sharing) but bet on the wrong language and execution
model. lpfx takes the same platform, picks the right language (GLSL),
and the right execution model (JIT).

## Chromatik / LX Studio (the lighting DAW)

By Mark Slee at Heron Arts, San Francisco. Originally **LX Studio** (~2013,
now end-of-lifed), succeeded by **Chromatik**. Yona has worked with the
team directly on a couple of installation projects in the past — this is
firsthand experience, not just web research.

Chromatik self-describes as **"a next-generation digital lighting
workstation"** that brings concepts from digital audio workstations and
modular synthesis into LED control. That framing matters — they put the
lighting-DAW idea on the map a decade before us. Anyone who's spent
serious time around large-scale art installations (Burning Man-adjacent,
festival, museum) knows this software.

**Important licensing nuance:** Chromatik (and LX before it) is
**source-available, not open source** — proprietary license from Heron
Arts LLC, source on GitHub for reference only. There's a generous free
tier (under $25k/year revenue) and commercial licensing above that. Mark
describes it as "developed largely in the *spirit* of open source," which
is fair — the source is readable, just not freely usable. Earlier
versions of LX Studio had broader open-source availability; that's
narrowed over time as it became Mark's commercial product. Worth being
accurate about this when comparing publicly. **lpfx is genuinely open
source — that's a real differentiator, not a marketing flourish.**

Their architecture, briefly (Java; package names from the public API):
- `lx.pattern` — generative animations (audio-, color-, form-, image-,
  strip-, texture-pattern subclasses).
- `lx.effect` — layer components with enable/disable.
- `lx.modulation` — automated modulation of parameter values.
- `lx.audio` — real-time audio analysis modulators.
- `lx.output` — Art-Net, sACN/E1.31, OPC, KiNET, DDP, OSC, MIDI.
- `glx.ui` — desktop UI for the mixer, patterns, modulators.

That structural shape — patterns + effects + modulation + audio + mixer
+ output protocols — **is the DAW model applied to lighting.** They've
been right about the model for a decade. Worth noting: their split of
**pattern (0-input generative)** vs. **effect (1-input transform)** is
the same arity-based classification we arrived at independently. Strong
signal that's the right primary axis.

**What they got right:**
- The DAW framing itself. Patterns ↔ instruments, modulation ↔
  automation, channels ↔ tracks, the whole vocabulary.
- Deep modulation + audio + MIDI integration as first-class.
- Protocol-rich output layer (Art-Net, sACN, OPC, etc.).
- Source-available engine + commercial app — a workable business pattern.
- A real community of large-scale-installation artists.

**Where lpfx differs (and where we earn our space):**
- **Runs on the chip itself.** Chromatik needs a desktop computer in the
  rig, then drives controllers over the network. lpfx targets a $3 MCU
  that *is* the controller. No computer. No network. No latency from a
  desktop being asleep when the show starts. That's a different shape of
  product entirely — wearables, single-fixture art, retail installs,
  anything where adding a laptop is a non-starter.
- **GLSL instead of Java.** Same logic as Pixelblaze-vs-us: GLSL is the
  cross-platform lingua franca with a 15-year corpus and AI fluency.
  Java patterns lock you into one ecosystem.
- **Genuinely open source**, not source-available. Permissive license,
  freely usable at any scale, no revenue cap.
- **Open shader format, not a plugin SDK.** ISF-shaped portable artifacts
  vs. Java classes that ship with the host.
- **AI as an authoring path** — Chromatik predates the LLM moment;
  authoring still assumes a Java developer.
- **Fixtures and mapping as first-class data, not code.** *(See below
  — this is the biggest single architectural differentiator.)*
- **Cheaper deployment tier.** Chromatik serves the high end well. The
  installation-grade market is small. The "$10 wearable that runs real
  shaders" market doesn't exist yet, and it can't until lpfx-shape
  systems exist.

### Yona's firsthand observation: the mapping problem

Direct quote from working with the LX Studio team on past projects:

> "Mega technical. Didn't have mapping at all — you had to hand-code all
> your fixtures in Java."

This is a significant pain point, and it's a real differentiator for
lpfx. Two issues bundled together:

1. **No built-in mapping.** "Mapping" in this world means the relationship
   between physical lamp positions in 3D space and the texture/effect
   coordinates being sampled. Every non-trivial LED installation needs
   it. Chromatik leaves it to you.
2. **Fixtures are code, not data.** When mapping does exist in a
   Chromatik project, it's expressed as Java classes the user writes.
   That gates the entire workflow on "you must be a Java developer."
   Artists, designers, and installers who can perfectly well describe
   their fixture in a TOML table cannot use Chromatik without a coder.

**lpfx's answer: fixtures are TOML.** A fixture is a list of polygons
(or points), each with 2D/3D coordinates and per-lamp output config.
Authoring a new fixture should be a 30-line file an artist writes by
hand, or imports from a CAD/SVG/photo-mapping tool. No code required.

This is also why the project layer (the 4th layer we just defined)
matters: that's where fixtures live. Putting fixtures into the
**deployment layer as data** — rather than into the effect/composition
authoring path as code — keeps the upper layers portable and the lower
layer accessible to non-developers.

**Implication for the architecture work:** when we draft `00-architecture.md`,
"fixtures are data, mapping is built in" is a load-bearing requirement,
not a nice-to-have. It's the single biggest reason lpfx is approachable
to non-coders that Chromatik isn't.

How to position publicly: with respect. Mark and team have been right
about the model and built the best lighting workstation in this lineage.
lpfx isn't a Chromatik replacement — it's the same model pushed down a
hardware tier into a friendlier authoring surface. Different segment,
shared vocabulary, friendly relationship.

## The DAW analogy (the mental model)

Worth stating plainly because Chromatik already proved this works for
lighting and it's the cleanest way to onboard new people:

A DAW (Digital Audio Workstation) is the software musicians use to make
recorded music — Ableton Live, Logic, Pro Tools, FL Studio. They all
share a four-layer structure that the industry has spent ~40 years
refining:

| DAW layer | What it is | lpfx parallel |
|---|---|---|
| **Plugin / instrument** | Code artifact that produces or transforms audio. Standardized format (VST, AU). Authored by coders. | **Effect** |
| **Preset** | A plugin instance with parameters saved. Authored by sound designers. | **Effect with bound params** / small composition |
| **Track + automation** | One source through a chain of plugins, with parameter curves over time. | **Composition** with modulators |
| **Song / arrangement** | Multiple tracks across a timeline with sections, transitions, tempo. | **Show** |
| **Project file** | The whole session: routing, audio interface, sample rate. Tied to specific gear. | **Project** (rig + fixtures + hardware + calibration) |

The structural pattern: **leaf artifact → arrangement → schedule →
deployment.** Plugins / presets / tracks / songs are portable (you can
share them); project files are tied to your specific gear.

**Five things DAWs already figured out that lpfx inherits:**

1. **Plugin formats made an ecosystem.** VST (1996) standardized "I
   produce audio with these typed parameters." Result: thousands of
   third-party plugins, a real market. The lpfx effect format is its
   VST. Get that interface right and a marketplace, sharing, AI-generated
   effects packs all become possible.
2. **Presets vs. plugins solved the coder/non-coder split.** Coders
   write plugins (hard, DSP knowledge). Sound designers write presets
   (easy, twist knobs, save). End users use presets (no knobs needed).
   This is the Yona / Luna / Customer split, solved 25 years ago.
3. **Automation is just modulators with another name.** Bind any
   parameter to a curve, an LFO, MIDI, or the audio signal itself.
4. **The timeline is a first-class object.** A song has verses,
   choruses, transitions. A show has the same shape.
5. **Project files are deployment-specific.** A `.als` file references
   plugins by ID; open it on a machine that's missing them and it tells
   you. Same model for lpfx projects.

**Where the analogy breaks** (so we don't follow it blindly):
- Audio is 1D over time; LEDs are 2D + time. We have a richer spatial
  layer (fixtures + geometry) that DAWs don't.
- DAWs assume desktop hardware. We're targeting $3 MCUs. Constraint
  shape is different.
- DAWs are mostly closed. We're open by default — that's the
  differentiation.

The closest DAW for lpfx-shaped thinking is **Ableton Live** — it's the
DAW built for live performance with clip launching, scenes, and tight
real-time modulation. Worth studying specifically when we design the
show layer.

## ISF as inspiration (and what it taught us)

[Interactive Shader Format](https://isf.video) — VIDVOX, ~2014. Single
shader file with a JSON manifest declaring typed inputs. Hosts (VDMX,
Resolume, etc.) parse the JSON and render UI from it. Several hundred
shaders in the public gallery.

ISF got the architecture right and bet on the wrong platform (desktop
GPUs) at the wrong time (before MCUs were ready). The model is right;
the platform was wrong. lpfx takes the model, points it at the platform
that's now ready, and inherits a decade of design work.

What we borrow:
- Shader + manifest of typed inputs → host renders UI.
- Multi-pass + persistent buffers for feedback (trails).
- Library + curation as the actual product (the format is the means).

What we change:
- Separate-file directory format instead of JSON-in-comment.
- TOML over JSON (human + agent friendly).
- Builtin/native step kind (escape hatch for stateful effects like
  fluid that shaders alone can't do efficiently on MCU).
- Composition layer above leaf effects (the bit ISF leaves to the host).

Plan: build an ISF importer for the desktop/wgpu path. Bootstraps the
gallery with hundreds of effects on day one. Imported effects work on
desktop, fail-fast with a clear error on MCU.

## Two-layer architecture (capsule)

Captured in more detail in the design docs, but the headline:

- **Effects** — single artifacts. Internal pipeline of shaders, compute,
  and builtins. Declare typed params. Coders + AI live here.
- **Compositions** — arrangements of effects. Stack, route, bind
  modulators (LFOs, audio, MIDI). Same TOML format; just one extra step
  kind that references another effect. Artists live here.
- **Customers** — see only the params of an effect or composition,
  rendered as UI sliders and color pickers.

Same artifact format throughout. Customers can install what artists
made; artists can install what coders made. No format translation
between layers.

## The bus framing (a more honest one-liner)

Late in the architecture work, a sharper framing emerged that's worth
saving for the technical pitch:

> **"Lightplayer is a typed signal bus that happens to also know how to
> draw beautiful things."**

The visual engine is the headline feature, but it sits on top of a
general signal-routing system. Sources publish typed signals (audio,
video, touch, MIDI, sensor data) onto a shared bus. Sinks subscribe.
The visual engine — Shows, Patterns, Effects, Mixers — is a peer
participant on that bus that happens to consume signals and produce
texture signals.

The implication is bigger than it sounds: **lightplayer can be useful
even with no Show at all.** A sensor-only ESP32 publishing microphone
audio over the network to a render-only ESP32 that just wraps a
texture-source onto LEDs is a complete deployment with zero authored
visuals. That's a real use case ("the mic node" + "the render node")
that most lighting systems can't express without bolting on a separate
protocol per signal type. A unified typed bus is a unification.

For pitch purposes: lead with "Shadertoy for LEDs" (more accessible).
Use the bus framing when talking to engineers, distributed-systems
people, or anyone who's bolted Art-Net + sACN + OPC + custom UDP
together and felt the pain.

## AI as the democratizing force (and as the meta-tool)

Two distinct roles AI plays in this story:

1. **AI as effect author.** Describe what you want in English → get a
   working effect. Non-coders get into the GLSL ecosystem without
   learning GLSL. This is the user-facing democratization.

2. **AI as engine builder.** The JIT, the fixed-point shader VM, the
   firmware, the fluid solver port — all built much faster than one
   person could have managed pre-AI. **lpfx as a project is itself a
   demonstration of what AI changes about what one person can ship.**

Both worth surfacing in announcement copy. The second is rarer and
more interesting to a technical audience.

## Personas served

| Persona | What they do |
|---|---|
| **Yona** (you) | Authors deep effects with custom shaders, builtins, and AI. |
| **Alex / Sean** (engineers, not LED specialists) | Modify someone else's effect with AI help. |
| **Luna / Clay / Raquel** (semi-technical artists who sell their work) | Compose existing effects, bind modulators, re-expose simple knobs. |
| **Customers of Luna et al.** | Pick from prebuilt; tweak the exposed knobs. |

Same artifact, same format, four surfaces.

## Quotable lines

Save these for marketing copy:

- "GLSL won 15 years ago. The microcontrollers finally caught up."
- "Shadertoy for LEDs, on a $3 chip, with AI as the author."
- "We took the model that's been right since 2014 and pointed it at the
  platform that became ready in 2024."
- "JIT is hard. I couldn't have done it in a reasonable time without AI
  help. That's the project, in microcosm — what one person can build
  has changed."
- "Interpretation is a hard ceiling for real 2D shaders. We measured ~6
  fps interpreted on a desktop, vs 30 fps JIT'd on a $3 chip."
- "The MCU LED world ran out of programmable expressivity around 2015.
  The graphics world had been waiting for hardware. The hardware
  arrived."
- "Chromatik proved a decade ago that lighting wants to be authored like
  music. We took that model and pushed it down a hardware tier — onto
  the chip itself."
- "It's a DAW for LED art — same four layers musicians have been using
  for forty years."
- "In Chromatik, your fixtures are Java classes. In lpfx, they're a
  TOML file an artist writes in an afternoon. That difference decides
  who can use the system."
- "We're genuinely open source — not 'source-available' with a revenue
  cap. That matters to anyone planning to build a business on top of
  this stack."

## Open story threads (revisit before announcing)

- The "fluid sim on ESP32" demo will probably want a video. Frame the
  video around the AI-author angle: "I described an underwater scene,
  it generated this." (Don't over-claim if the AI-author tooling isn't
  shipped yet.)
- Pixelblaze comparison should be friendly + factual, not competitive.
  Ben is a friend.
- Chromatik comparison should be respectful — Mark Slee pioneered the
  lighting-DAW framing and built the best tool in that lineage. Position
  lpfx as the same model pushed to a hardware tier they don't target,
  not as a replacement. Worth a direct outreach to Mark before any
  public announcement that mentions Chromatik by name.
- ISF acknowledgment should be generous. Credit VIDVOX directly.
- The "Rust + JIT + fixed-point on a no-FPU RISC-V part" story is its
  own technical post worth writing for the hacker audience. Different
  audience from the artist-facing pitch.
- Decide: is the project name "lightplayer" / "lpfx" / something
  newer? The current names are working titles.
