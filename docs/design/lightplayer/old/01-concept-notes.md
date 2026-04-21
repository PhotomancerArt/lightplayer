Detailed entries follow.

---

## Two-tier discipline

The vocabulary is organized in two tiers:

- **Data tier** — the abstract mechanism. Signal, Source, Sink,
  Transformer, Bus, Channel, Module, Parameter, Binding. These describe
  _how data moves_. Mostly invisible to authors; visible to engine
  implementers and runtime designers.
- **Domain tier** — the named artifacts. Visual (Pattern, Effect,
  Mixer, Show), Layout, Fixture, Lamp Sink, Rig, Project. These are
  _what people author and point at_. They are concrete manifestations
  of the data tier roles.

When a domain term and a data term seem to overlap, both apply: a Show
_is a_ Module (data tier) and _contains_ Visuals (domain tier). A Rig
_is a_ Module and _contains_ Sources, Sinks, and Layouts.

The two-tier discipline keeps the data tier from leaking into authoring
("everything is a Node graph") while keeping the domain tier from
fragmenting ("we have ten special-cased participants").

---

## Data tier

### Signal

Typed data flowing through the system. Built from WGSL primitive types
(`f32`, `vec3`, `Texture`, `struct`, array) plus named composites:
`Audio`, `Video`, `AudioFFT`, `XYPoint`, `MidiEvent`, `OscEvent`,
`LampArray`, `Float`, `Bool`, `Pulse`. Continuous or event-stream.

Signals always have a type. Bindings between signals are type-checked
at compose time.

### Source

Anything that produces a Signal. Two flavors:

- **Hardware Source** — a real device or interface: camera, microphone,
  MIDI port, GPIO pin, OSC server, IMU.
- **Software Source** — a synthesized or derived stream: LFO, beat
  clock, envelope follower, file-backed playback.

A Source publishes onto a typed Channel. Sources are not files
themselves — they are entries inside a Module (typically a Rig, but
software sources can also live in Shows or Visuals).

### Sink

Anything that terminally consumes a Signal. The end of a chain — sinks
do not republish. Examples: LED hardware output (Lamp Sink), future
audio output (speaker), recorder, network broadcast.

A Sink subscribes to a typed Channel. Like Sources, Sinks live inside
Modules (typically Rigs).

### Transformer

Anything with both Signal inputs and Signal outputs. Not a separate
file type — most transformers are Visuals (texture in → texture out)
or Layouts (texture in → lamp array out). A few would be Sources with
inputs (envelope follower: audio in → float out).

The data tier does not require an explicit "Transformer" artifact.
Transformers manifest as the appropriate domain artifact for their job.

### Bus

The typed channel space connecting all Modules in a Project.
**Implicit** — the bus is derived from the union of all channel
declarations across modules in the project. It is not authored as a
file; it emerges.

The bus is responsible for compose-time validation: missing publishers,
multiple publishers on a single-producer channel, type mismatches,
convention violations.

The bus may be local-process or network-transparent (the latter is a
v2+ concern, but the model supports it).

### Channel

A typed, named address on the Bus. Format:

```
<type>_<direction>/<index>
```

Direction is relative to the visual pipeline (the Show layer) — `in`
means raw from the world, `out` means processed by a visual. Types
that are inherently unidirectional (touch, IMU, GPIO buttons) skip the
direction suffix.

Examples:

- `audio_in/0` — raw mic or audio file
- `audio_out/0` — synthesized audio from a show
- `video_in/0` — raw camera or video file
- `video_out/0` — texture from a show
- `lamps_out/0` — lamp array from a layout
- `touch/0` — combined touch points (no direction; unidirectional)
- `midi_in/0`, `midi_out/0` — MIDI is bidirectional

### Module

A self-contained Bus participant. Declares typed channel sources
(publishes onto) and channel sinks (subscribes from). The **data-tier
role**; in the domain tier, every module is either a Show or a Rig.

Two modules can be swapped if they declare matching channels.
Portability is at the module boundary.

### Parameter

Typed config Property on an Entity (Show, Pattern, Effect, Mixer,
Layout, etc.). Has a default value, a type, and optional UI hints
(range, label, render-as-knob, hidden). **Authored, not streamed.**
Distinct from Signal.

A Parameter is a **Property**, not an Entity. It's addressed via the
owning Entity's EntitySpec plus a PropPath: e.g., the `speed`
parameter of `main.show/fluid.vis` is the EntityPropSpec
`main.show/fluid.vis#speed`. The same form addresses deep-nested
config like `dome.rig/sector4.fixture#config.spacing`.

A Parameter's value at any frame comes from one of two paths:

1. **Direct write** — a user dragged a Control, the Control wrote the
   new value directly into the Parameter. No bus involvement.
2. **Binding pull** — the Parameter has a Binding pointing at a Signal
   on the bus (or another Parameter); the value is computed from the
   bound source each frame, optionally with a transform.

When no Binding exists, the Parameter holds the last directly-written
value. When a Binding exists, it overrides direct writes (or coexists
per the binding's mode — TBD).

UI hints on a Parameter feed the auto-derivation of Controls. A
Parameter with `hidden = true` has no Control; a Parameter with
`render_as = "color"` gets a color picker; etc.

### Binding

A connection from a Signal (or another Parameter) to a Parameter,
allowing the parameter to be driven by the source each frame.

A Binding has:

- **Target** — an EntityPropSpec
  (`main.show/fluid.vis#speed`)
- **Source** — either a Channel address (`audio_in/0`) or another
  EntityPropSpec
- **Transform** (optional) — scale, offset, smoothing, curve, deadband
- **Fallback** (optional) — value or behavior when the source is silent

Bindings live in the manifest of the artifact whose Parameter is being
bound. Project-level bindings live in the project manifest.

The canonical authoring UX is **click-to-learn** (DAW-style): user
clicks the target Control, system enters learn mode, user wiggles a
physical control or moves a software signal, system identifies the
most active source and writes the binding. Same flow works for AI
("bind speed to the kick drum band").

### Modulator

Any Source whose typical use is driving Parameters via Bindings: LFO,
audio-band amplitude, beat clock, smoothed CC, envelope follower.

A Modulator is just a Source plus a usage convention. Not a separate
data-tier concept.

---

## Domain tier

### Visuals

#### Visual

Umbrella term for any artifact that produces a Texture Signal. Five
kinds, distinguished by **arity** (number of primary texture inputs)
and **statefulness over time**:

|                       | 0-arity (source) | 1-arity (transform) | 2-arity (blend) | N-arity (combine) |
| --------------------- | ---------------- | ------------------- | --------------- | ----------------- |
| Stateless             | Pattern          | Effect              | Transition      | Mixer             |
| Stateful + time-aware | —                | —                   | —               | Show              |

Transition and Mixer overlap structurally — a 2-input Mixer with a
`progress` parameter _is_ a Transition. The split is by **contract and
intent**, not exclusively by arity: Transition has a conventional
contract that Shows depend on; Mixer is free-form.

Visuals are recursive — a Visual's pipeline can reference other
Visuals.

A Visual is **not** the same as a Module. A Visual is a unit of
texture-producing logic. A Module is a bus participant. A Show is both
(the only Visual that is also a Module).

#### Pattern

0-arity Visual (source). Produces a texture from nothing — driven by
parameters and bindings only. Schedulable directly in a Show.

The basic content unit. _Audio analogy: a synth._

Manifest: `*.pattern.toml`.

#### Effect

1-arity Visual (transform). Takes one primary texture input, produces a
transformed texture output. Cannot be scheduled directly in a Show
because it needs a source.

_Audio analogy: an effects pedal._

Manifest: `*.effect.toml`.

#### Transition

2-arity stateless Visual with a conventional `progress: f32` parameter
in `[0, 1]`. At `progress = 0` the output is fully input A; at
`progress = 1`, fully input B; in between, blended per the
transition's logic (cross-dissolve, wipe, slide, stinger, etc.).

First-class because the Show layer depends on Transitions to switch
between candidate visuals. Every Show kind (Live, Playlist, Timeline)
references Transitions by name.

A Transition is structurally a 2-input Mixer with a fixed parameter
contract. The split exists because Shows need to ask for "a
Transition" with confidence about its shape, not "a 2-input Mixer that
happens to expose a `progress` knob."

_Video analogy: cross-dissolve, wipe, slide, stinger. Same word
across Final Cut, Premiere, Resolve, OBS, Resolume._

Manifest: `*.transition.toml`.

#### Mixer

N-arity (N≥2) stateless Visual. Combines multiple texture inputs
simultaneously into one output. Free-form contract — any parameters,
any blend logic.

Used for compositing (screen-blend two patterns to layer them, sum
four reactives) where the relationship between inputs is spatial or
additive rather than temporal.

Stateless about its primary inputs — does not maintain memory across
frames about which inputs it received.

_Audio analogy: a mix bus._

Manifest: `*.mixer.toml`.

#### Show

N-arity, stateful, time-aware Visual. Selects from candidate visuals
over time, handles transitions, owns parameter bindings. Schedulable
in another Show (recursive).

Also a Module — has channel declarations on the bus.

Three kinds, distinguished by loading model and selector shape:

- **Live Show** — all candidates loaded simultaneously; selection by
  self-reported priority plus operator override. High memory cost,
  instant reaction. (Burning Man stage with audio/touch/fallback.)
- **Playlist Show** — bounded loading (current + next pre-rolling);
  selection by manual cue, sequence, or schedule. Conserves memory.
- **Timeline Show** — ordered sequence of `(visual, transition)` pairs,
  wall-clock anchored. Transitions are authored in place.

Manifest: `*.show.toml`.

### Hardware artifacts

#### Layout

A 2D pixel canvas with Fixtures placed on it. Owns:

- Canvas resolution
- Fixtures (placement, LED specs, calibration, wiring)
- Channel subscription (which texture channel it consumes, default
  `video_out/0`)

Also a Transformer: texture in → lamp array out. Publishes to a
`lamps_out/N` channel that Lamp Sinks consume.

Lives inside a Rig.

Manifest: `*.layout.toml`.

#### Fixture

A placed instance of a physical light unit within a Layout. Holds:

- Geometry (lamp positions in canvas space — points or polygons)
- Color format and order (`rgb`, `rgbw`, color order)
- Calibration (gamma, color correction)
- Wiring entries (which Lamp Sink, which universe, which offset)

Lives inside a Layout. May be exported as `*.fixture.toml` for reuse,
but is normally inlined in the layout file.

#### Lamp Sink

A hardware delivery channel for lamp data. Examples:

- Local: WS2811 over RMT, SPI to APA102
- Networked: Art-Net node, sACN universe block, OPC server, KiNET, DDP

Subscribes to a `lamps_out/N` channel. Lives inside a Rig.

Manifest: `*.sink.toml` (was `*.output.toml` in earlier drafts; rename
pending).

#### Source artifact

A declared signal producer. Hardware sources are tied to a physical
device entry; software sources are pure declarations.

Sources are not standalone files in v1 — they are entries inside the
Module (Rig or Show) that owns them.

### UI artifacts

#### Control

A UI element derived from a Parameter or a bus channel. **Not a
Source** — Controls do not publish onto the bus. They are the
_rendering_ of a Parameter as something a user can interact with.

Two derivation sources:

1. **Parameter-derived** — every Parameter's UI hints
   (`render_as = "knob"`, `range`, `label`) auto-generate a Control.
   The default UI for any artifact.
2. **Bus-derived (debug)** — every Channel on the bus implicitly gets
   a viewer Control: texture → thumbnail, audio → meter/waveform,
   float → scope, event → activity log. Free observability.

When a user manipulates a parameter-derived Control, it writes
directly to its Parameter (no bus involvement). When a Parameter has
a Binding active, the binding takes precedence (or coexists per
binding mode).

Controls are addressed by EntityPropSpec (e.g.,
`main.show/fluid.vis#speed`) — the same address as the Parameter
they render.

Controls are **not yet authored as standalone artifacts.** They
emerge from Parameter declarations and bus introspection. Future:
custom standalone Controls in Panels.

#### Panel

A curated arrangement of Controls. Lets an author pick a subset of
Controls from across the Parameter tree, give them custom labels and
layout, and present them as a coherent UI surface (Basic /
Advanced / Performer).

Panels are _optional_ — the auto-derived UI from Parameters and bus
channels works without any Panel being authored. Panels are the
customization layer.

Panels live primarily in Shows (the Show's UI). A Project-level
Panel is also possible (the install operator's master surface).

**Concept only in v1.** Not yet a manifest type.

#### Control Surface

A runtime rendering target that hosts Controls and Panels:
touchscreen, web client at `/panel/foo`, hardware MIDI controller,
Ableton Push, iPad app.

One Control Surface can host multiple Panels (switch between them).
One Panel can be rendered on multiple Surfaces (touchscreen + web
mirror).

Surfaces may also publish back to the bus as Sources — a MIDI
controller's knobs publish CC values; a touchscreen drag publishes
an XYPoint stream. The Surface's role as Source is separate from its
role as a Control renderer; the same physical device can play both
roles.

**Concept only in v1.** Detailed surface registry and rendering
contract deferred.

### Composition artifacts

#### Rig

A Module containing hardware-anchored Sources, Lamp Sinks, Layouts, and
optionally global software Sources (a project-wide LFO). Portable: a
Rig is a deployment-independent description of a hardware bundle.

A Project may include any number of Rigs. There is no enforced
"InputRig" / "OutputRig" partition at the data model level — a Rig may
contain only sources, only sinks, or both. The convention of splitting
control-side and render-side rigs is a UX/operator pattern, not a
schema rule.

Manifest: `*.rig.toml`.

#### Project

Top-level deployment artifact. Lists:

- The Modules in the deployment (Rigs + Shows)
- Channel binding overrides (per-device deviations from convention)
- Project-level metadata (name, target hardware, install context)

The unit of install. Tied to a specific hardware setup because of the
Rigs it references; portable in that you can re-target by swapping a
Rig.

Manifest: `*.project.toml`.

---

## Concepts

### Identity (overview)

lpfx separates **two trees** and uses **three reference forms** plus
a runtime ID. The trees:

- **Entity tree** — runtime composition. A Show _contains_ Patterns;
  a Layout _contains_ Fixtures; a Rig _contains_ Sources and Sinks.
  Author-driven nesting.
- **Property tree** — value structure inside one Entity. A Fixture
  has a `config` struct with a `spacing` field; a Pattern has a
  `palettes` map. Schema-driven.

The references:

| Form               | Identifies                            | Form syntax                                                             |
| ------------------ | ------------------------------------- | ----------------------------------------------------------------------- |
| **Specifier**      | An Artifact (file)                    | `./fluid.pattern.toml`, `lib/std:rainbow`, `{ git = ..., path = ... }`  |
| **EntitySpec**     | One Entity at runtime                 | UID (`7Kp2MqZ`) or entity-tree path (`main.show/fluid.vis`)             |
| **PropPath**       | A value inside one Entity             | `config.spacing`, `wiring[0].universe` — lp-core's existing path syntax |
| **EntityPropSpec** | A specific value on a specific Entity | `(EntitySpec, PropPath)` — `main.show/fluid.vis#speed`                  |

Plus the runtime ID:

- **UID** — base-62, ~11 chars, runtime-only. Engine-internal handle.
  Never in TOML.

The flow: a TOML manifest uses a **Specifier** to import an Artifact;
the loaded Artifact becomes an Entity at a position in the entity
tree. **EntitySpec** addresses that Entity (by entity-path or, at
runtime, by UID). **PropPath** addresses values within an Entity.
Bindings target an **EntityPropSpec** — `(EntitySpec, PropPath)` —
which is the address of a specific value to drive.

### Specifier

A string in TOML that resolves to an Artifact at load time. Cargo /
JS-modules / npm style.

Forms:

```toml
spec = "./fluid.pattern.toml"                # local file, relative to manifest
spec = "/abs/path/fluid.pattern.toml"        # absolute path
spec = "lib/standard:rainbow"                # library reference (future)
spec = { git = "https://...", rev = "abc", path = "..." }   # git source (future)
```

Resolution happens at Project load time. The resolved Artifact is
loaded and instantiated as an Entity in the current Project's entity
tree. The runtime assigns the new Entity a UID.

(Term borrowed from JavaScript / WHATWG: `import "./foo.js"` is a
"module specifier." Same concept, same name.)

### EntitySpec

Identifies one Entity. Two forms, both valid wherever an EntitySpec
is expected:

**UID form**: `7Kp2MqZ4f9w` — runtime-only, used by the engine
internally.

**Entity-path form**: `main.show/fluid.vis` — used in TOML and
human-facing surfaces. Slash-separated, each segment is
`<name>.<type>` matching the artifact filename order. The type
suffix is optional when the name alone is unambiguous in scope.

Examples:

- `main` — short form, name unambiguous in scope
- `main.show/fluid.vis` — typical form: each segment carries its type
- `dome.rig/main.layout/sector4.fixture` — three levels deep
- `main.show/fluid.vis/palette.vis` — visual-inside-visual

The `.<type>` suffix matches the Artifact's filename suffix:
`fluid.vis.toml` → `fluid.vis`, `dome.rig.toml` → `dome.rig`,
`sector4.fixture.toml` → `sector4.fixture`. Type suffixes:

- `.show` — Show
- `.vis` — Visual (Pattern, Effect, Mixer, Transition — type
  refinement determined by manifest contents, not filename)
- `.rig` — Rig
- `.layout` — Layout
- `.fixture` — Fixture
- `.sink` — Lamp Sink (or other Sink)
- `.source` — Source
- `.project` — Project

The slash-based form maps directly to filesystem layout. With the
**directory-as-entity** convention (v1, local-only):

```
<project-dir>/
  main.show/
    entity.toml          # the show
    fluid.vis/
      entity.toml        # the pattern
      fragment.glsl
  dome.rig/
    entity.toml
    sector4.fixture/
      entity.toml
```

EntitySpec `main.show/fluid.vis` corresponds to the on-disk path
`<project>/main.show/fluid.vis/entity.toml`. The path you write in
TOML _is_ the path on disk, modulo the `entity.toml` leaf.

(For trivial leaf entities — a one-line color sub-pattern, say —
flat-file form `name.<type>.toml` is equivalent to a directory with
just an `entity.toml`. v1 may support both; spec form is the same.)

### PropPath

A path into an Entity's value tree. Dotted fields with bracket
indices for arrays — **the existing `LpsPathSeg` syntax from
`lp-shader/lps-shared/src/path.rs`**, reused as-is.

Examples:

- `speed`
- `config.spacing`
- `wiring[0].universe`
- `palettes.sunset.colors[3]`
- `lights[3].color.r`

Grammar: identifier-or-`[index]` segments separated by `.`. Existing
parser + walker in lp-core handles parse, get, set.

### EntityPropSpec

The address of a specific Property on a specific Entity:
`(EntitySpec, PropPath)`. The standard form for binding targets and
sources.

**Compact string form** (separator `#`, URL-fragment style):

```toml
target = "main.show/fluid.vis#speed"
source = "dome.rig/lfo-1.source#current_value"
```

**Structured table form** (editor-friendly):

```toml
target = { entity = "main.show/fluid.vis", prop = "speed" }
source = "audio_in/0"
```

Both forms are accepted (Cargo-style flexibility). Compact for
hand-writing, structured for editor-generated.

`#` was chosen because URLs use it for "the part within the resource"
— exact semantic match.

### UID

A short alphanumeric identifier for an Entity at runtime. Base-62,
~11 characters at 64 bits of entropy (the `unique` tier from the
project's existing `Base62Uid` helper). Example: `7Kp2MqZ4f9w`.

Properties:

- **Runtime-only.** UIDs are generated by the runtime when an Entity
  is instantiated. They never appear in authored TOML.
- **Stable for the runtime session.** Survives renames, moves,
  re-resolution.
- **Collision-safe within a Project.** 64-bit entropy is plenty.
- **Readable enough for debug.** Log lines, error messages, and the
  debug Panel can show UIDs without overwhelming a human.

UIDs are the primary internal handle for engine code: lookup tables,
binding resolution caches, undo stacks, snapshot diffs. Entity-paths
are the user-facing handle; UIDs are the engine's.

### Entity

A runtime instance — a node in the entity tree with a UID, a Name,
and a position. Examples: Modules, Visuals (any kind), Layouts,
Fixtures, Sources, Sinks, Bindings, Controls, Panels.

**Parameters are not Entities.** A Parameter is a Property of its
owning Entity — addressed via `(EntitySpec → owner, PropPath →
parameter-name)`. Parameters share the lifetime and identity of their
owning Entity.

Entities are runtime instances, not files. The same Artifact can
appear as multiple Entities (a Pattern referenced twice in a Timeline
Show is two Entities sharing one Artifact, with two different UIDs).

(Term borrowed from lp2014. Fits the DDD usage of "Entity" — a thing
with stable identity over time.)

### Property

A typed value attached to an Entity, addressed by PropPath. Includes
Parameters (config with UI hints), but also any other addressable
value on an Entity (a Source's `current_value`, a Sink's
`last_sent_at`, etc.).

Properties don't have UIDs. They share their owning Entity's UID and
are addressed by `(EntitySpec, PropPath)`.

### Artifact

An authored file on disk: `*.pattern.toml`, `*.effect.toml`,
`*.show.toml`, `*.rig.toml`, `*.project.toml`, etc. The class to
Entity's instance.

Artifacts are portable, sharable, version-controlled. A Pattern
Artifact in a library can be referenced (via Specifier) from many
Projects; each reference instantiates an Entity within that Project.

The Artifact / Entity distinction matters when:

- The same Artifact is instantiated multiple times in one Project
  (each instance is a different Entity with a different position in
  the tree, a different UID).
- A Project shares Artifacts with another Project (the Artifact has
  cross-project meaning; the Entity does not).

For simple cases where each Artifact is used exactly once, the
distinction collapses and you can think of them as the same thing.

### Name

The human-readable label of an Entity, used as its leaf component in
the entity tree.

Default: the basename of the Specifier's path (`./fluid.pattern.toml`
→ name `fluid`). Filesystem-centric, Cargo-flavored — the file
system gives names for free.

Override: any Entity declaration in TOML can specify
`name = "..."` to give a different name (essential when instantiating
the same Artifact multiple times to avoid name collisions).

Inline-declared Entities (a Source declared directly in a Rig
manifest, with no separate file) write the name explicitly:
`[[sources]]\nname = "camera1"`.

Names follow filesystem-friendly grammar: `[A-Za-z0-9_-]+`. No spaces,
no dots (dots are the entity-path / prop-path separator).

### Arity

The number of primary texture inputs a Visual takes. `0` = Pattern,
`1` = Effect, `2` stateless with `progress` parameter = Transition,
`N≥2` stateless free-form = Mixer, `N≥1` stateful + time-aware = Show.
Defines what can compose with what (an Effect cannot be scheduled in a
Show; only 0-arity-net Visuals can).

### Convention (channel)

Default channel-binding rules. Hardware Sources publish to `*_in`,
Shows publish to `*_out`, Layouts consume `*_out`, Lamp Sinks consume
`lamps_out`. Combiner behavior is per-signal-type (touch combines,
audio mixes, video selects-by-index).

Most projects work without writing a single override.

### Override (binding)

A per-device or per-module deviation from convention. Lives on the
device whose binding is being overridden. The mechanism that handles
the rig-to-rig case (a passthrough sink subscribing directly to an
`*_in` channel) and any other non-default wiring.

For type-mismatch reroutes (audio → texture → lamps), the answer is
not an override — it's a tiny passthrough Show. The bus does not do
type conversion.

### Wiring

The concrete connection record between a publisher and a consumer.
Implicit (convention-driven) or explicit (override). Compose-time
resolved and validated.

### Manifest

The `*.toml` file for any artifact. File type indicated by suffix:

| Suffix              | Artifact              |
| ------------------- | --------------------- |
| `*.pattern.toml`    | Pattern               |
| `*.effect.toml`     | Effect                |
| `*.transition.toml` | Transition            |
| `*.mixer.toml`      | Mixer                 |
| `*.show.toml`       | Show                  |
| `*.layout.toml`     | Layout                |
| `*.fixture.toml`    | Fixture (when reused) |
| `*.sink.toml`       | Lamp Sink             |
| `*.rig.toml`        | Rig                   |
| `*.project.toml`    | Project               |

Companion files: `*.glsl` / `*.wgsl` for shader source, binary blobs
for data tables.

### Scope (of a software source)

Where a software source (LFO, beat clock, etc.) is declared:

- **Rig-scoped** — visible to all consumers in the project. Lives in a
  Rig. Use case: a global tempo LFO.
- **Show-scoped** — exists only while the show is loaded. Lives in a
  Show. Use case: a modulator the show owns and tears down with itself.
- **Visual-scoped** — private to one visual. Lives in the visual's
  manifest. Use case: a pattern's internal hue-cycle LFO.

### Recursion

Visuals can reference other Visuals (a Show containing Patterns; a
Show containing another Show; an Effect's config slot accepting a
Pattern). Bounded — no cycles, depth limit enforced at compose time.

### Validation

Compose-time check, never runtime. The bus, parameter system, and
artifact loaders cooperate to verify:

- Every channel consumer has at least one publisher (warn / fail).
- Every non-combiner channel has at most one publisher (error).
- Publisher and consumer types match (error).
- Visuals in a Show net to 0-arity (error).
- Parameter bindings are type-compatible (error).
- Convention violations (warn, suppressible).

A Project that loads is a Project that runs.

---

## Deferred

Terms named here but not yet fully designed. Each deserves its own
architecture document later.

- **Combiner-by-type rule**: per-signal-type spec for what happens
  when multiple sources publish to one channel (touch merges, audio
  sums, video selects).
- **Modulator binding syntax**: how a binding is expressed in TOML —
  inline expression, separate `[bindings]` table, or both.
- **Snapshot / tween-cue system**: parameter snapshots and
  inter-snapshot tweening (the lp2014 `TweenManager` equivalent).
- **Power budget**: rig-level power management beyond per-fixture and
  per-sink caps.
- **Bus transport**: local-process vs. network-transparent. v1 is
  local; the model supports network as a transport choice.
- **Custom Panels**: hand-curated arrangements of Controls. v1 has
  auto-derived UI only; custom Panel manifest format deferred.
- **Control Surface registry**: how Surfaces are declared, discovered,
  and bound to Panels (touchscreen at GPIO-X, web server at port-Y,
  MIDI controller GUID-Z).
- **Hardware tier system**: how Visuals declare minimum tiers and how
  runtimes refuse incompatible loads.
- **Module-contents abstract container**: Sources, Sinks, Visuals,
  Controls, Bindings all live inside Modules and share shape (have
  inputs, outputs, configs, paths). Likely deserves a unifying
  abstraction. Term TBD; "node" rejected (overloaded with lp-core
  internals); "entity" claimed for the runtime-instance role above.
  Returning to this when it earns its keep.
- **EntitySpec scope rules**: when the `<name>.<type>` suffix is
  required vs. optional. Probably "required iff name alone collides
  in the resolution scope," but the precise scope (sibling-only vs.
  whole-tree) needs writing down.
- **Specifier resolution rules**: search order for relative-to-manifest
  vs. project-relative vs. library lookups; library namespace shape;
  git/url-source spec format.
- **Array slot semantics**: how to address one entry of an indexed
  collection in EntitySpec form (named vs. ordinal —
  `transitions.crossfade` vs `transitions[0]`).

---

## Renaming history

For traceability when reading older docs:

| Old term             | Current term                | Notes                           |
| -------------------- | --------------------------- | ------------------------------- |
| Module (for visuals) | Visual                      | "Module" reused at higher level |
| Effect (umbrella)    | Visual                      | Effect now means 1-arity only   |
| Composition          | Show (or recursive Visual)  | Collapsed                       |
| Output (artifact)    | Lamp Sink (or Sink)         | Generalized                     |
| OutputRig / InputRig | Rig                         | One artifact, N instances       |
| Module (Rig+Show)    | Module                      | Data tier role only             |
| Control (as Source)  | Control (as UI over Param)  | Not on the bus; derived UI      |
| Path (entity addr)   | EntitySpec + PropPath split | Two trees, two paths            |
| Parameter (Entity)   | Parameter (Property)        | Properties live on Entities     |
