# Quantity model — LightPlayer domain primitives

This document is the **single source of truth** for how data types,
semantics, constraints, and composition are modeled in the
LightPlayer domain. All implementations (`lp-domain`, lpfx, the
TOML loader, schema generation, runtime binding, debug viewers)
must conform.

Sister documents: [`color.md`](../color.md) (color-specific
contract), [`q32.md`](../q32.md) (int-only fixed-point fallback),
[`domain.md`](./domain.md) (broader vocabulary: Visuals, Rigs,
Modules, Bus).

## TL;DR

1. Five layers separate concerns cleanly:
   - **`LpsValue`** — raw structural data (bytes, no semantics).
     Lives in [`lps-shared`](../../../lp-shader/lps-shared).
   - **`LpsType`** — structural type of an `LpsValue`. Also
     `lps-shared`.
   - **`Kind`** — semantic identity of a value (Frequency, Color,
     Texture, Audio, ...). Defines storage recipe, dimension,
     default constraint, default presentation, default bind.
   - **`Constraint`** — what values are *legal* for a Slot
     (range, choices). Domain truth, not validation glue.
   - **`Slot`** — recursive declaration: a `Shape` (Scalar /
     Array / Struct) plus per-use metadata (default, label, bind,
     presentation).
2. **Every Slot can produce a default value.** Scalar Shapes
   carry a mandatory `default: ValueSpec`; composed Shapes
   (`Array`, `Struct`) carry an `Option<ValueSpec>` override and
   derive a default from their children when absent. For
   value-Kinds the spec is `Literal(LpsValue)`; for opaque-handle
   Kinds (Texture, future Audio/Video) it's a recipe the loader
   materializes.
3. **The bus is implicit.** Channels are Kind-typed, derived from
   the first binding; subsequent bindings to the same channel must
   declare the same Kind or compose-time error.
4. **Composition shapes mirror what WGSL/GLSL can express:**
   `Scalar | Array | Struct`. No tuples, no sum types.
5. **F32 is canonical numeric type.** Q16.16 (`Q32`) is the int-only
   firmware fallback. See [`color.md`](../color.md) for the full
   precision contract.

## 1. Why this matters

LightPlayer is a typed-signal-bus system that happens to draw
beautiful things. The domain model has to bear three loads at once:

- **GPU truth.** Every Kind's storage projects directly to a
  `LpsType` that compiles down to a real `vec3` / `mat3` / `array`
  the shader sees. No impedance mismatch at the GPU boundary.
- **Author intent round-trip.** A user picks a color in OKLCH,
  saves, reopens — they should see OKLCH. A user writes a Visual
  with `range = [0, 5]`; that's the *legal* range, not just a UI
  hint. The model preserves intent across save/load.
- **Embedded-friendly.** ESP32-class targets (`no_std + alloc`,
  Q32 fallback for int-only firmware variants, fixed-size arrays
  for collections). The model can't assume `std` or unbounded
  allocation.

The five-layer split below is what falls out of taking those three
loads seriously.

## 2. The five layers

| Layer        | Purpose                                                                       | Lives in       |
| ------------ | ----------------------------------------------------------------------------- | -------------- |
| `LpsValue`   | Raw structural data. F32/I32/Bool/Vec*/Array/Struct. No semantics.            | `lps-shared`   |
| `LpsType`    | Structural type of an `LpsValue`. Mirrors the variant set.                    | `lps-shared`   |
| `Kind`       | Semantic identity. Defines storage recipe, dimension, defaults.               | `lp-domain`    |
| `Constraint` | What values are legal (range, step, choices). Domain truth.                   | `lp-domain`    |
| `Shape`+`Slot` | Recursive composition. Shape is structural (Scalar/Array/Struct); Slot adds metadata. | `lp-domain` |

`LpsValue` and `LpsType` come from `lps-shared` (the foundational
GLSL-type layer used by the shader compilers). `lp-domain` adds the
semantic / metadata layers on top.

```
                                       ┌─────────────────────────┐
                                       │ Slot                    │
                                       │  ├─ shape: Shape        │
                                       │  ├─ default: ValueSpec  │
                                       │  ├─ label, description  │
                                       │  ├─ bind: Option<...>   │
                                       │  └─ present: Option<...>│
                                       └────────────┬────────────┘
                                                    │
                                  ┌─────────────────┴─────────────────┐
                                  │                                   │
                          ┌───────▼────────┐                          │
                          │ Shape          │                          │
                          │  ├─ Scalar     │                          │
                          │  ├─ Array(Slot)│                          │
                          │  └─ Struct[Slot]                          │
                          └───────┬────────┘                          │
                                  │                                   │
                  ┌───────────────┴───────────────┐                   │
                  │                               │                   │
          ┌───────▼────────┐              ┌───────▼────────┐          │
          │ Kind           │              │ Constraint     │          │
          │  ├─ Frequency  │              │  ├─ Free       │          │
          │  ├─ Color      │              │  ├─ Range{...} │          │
          │  ├─ Texture    │              │  └─ Choice{...}│          │
          │  └─ ...        │              └────────────────┘          │
          └───────┬────────┘                                          │
                  │                                                   │
                  │ storage()                                         │
                  ▼                                                   │
          ┌────────────────┐                                          │
          │ LpsType        │ ◄───── lps-shared (GPU truth)            │
          │ LpsValue       │                                          │
          └────────────────┘                                          │
                                                                      │
                                          ┌───────────────────────────▼───┐
                                          │ ValueSpec (the "default")     │
                                          │  ├─ Literal(LpsValue)         │
                                          │  └─ Texture(TextureSpec)      │
                                          └───────────────────────────────┘
```

## 3. `Kind` — the semantic identity layer

A `Kind` answers "what category of thing is this value?" Most are
scalars; some are structured values with an interpretation; some
are opaque handles. The Kind set is **open** by design — new Kinds
land as the example corpus grows.

### Open enumeration

```rust
pub enum Kind {
    // Scalars (Dimensionless)
    Amplitude,    // [0, 1] strength of a signal
    Ratio,        // [0, 1] fraction / proportion
    Phase,        // [0, 1) wrapping cycle position
    Count,        // unsigned integer count
    Bool,
    Choice,       // discrete enum

    // Scalars (with Dimension)
    Instant,      // F32 seconds since some epoch (was "Time")
    Duration,     // F32 seconds (a span)
    Frequency,    // F32 Hz
    Angle,        // F32 radians (free, can exceed 2π)

    // Structured value-Kinds
    Color,           // see color.md
    ColorPalette,    // see color.md
    Gradient,        // see color.md
    Position2d,      // Vec2
    Position3d,      // Vec3

    // Bulk / opaque-handle Kinds
    Texture,         // GPU-resident pixel buffer (see §6)
    // Audio, AudioFft, AudioLevel, Beat, Touch, Motion, ...
    //   land as concrete examples drive them in M3+
}
```

### Per-Kind contract

Every `Kind` provides:

```rust
impl Kind {
    /// The structural LpsType the GPU/serializer sees.
    pub const fn storage(self) -> LpsType;

    /// Commensurability class.
    pub const fn dimension(self) -> Dimension;

    /// Natural domain of the Kind (e.g. Amplitude is [0, 1]).
    pub const fn default_constraint(self) -> Constraint;

    /// Default rendering for a Slot of this Kind.
    pub const fn default_presentation(self) -> Presentation;

    /// Conventional bus binding for input Slots of this Kind.
    pub const fn default_bind(self) -> Option<Binding>;
}
```

### Storage recipes (selected)

Color-family recipes are locked in [`color.md`](../color.md). The
others:

| Kind           | Storage                                                                                                          |
| -------------- | ---------------------------------------------------------------------------------------------------------------- |
| Scalar Kinds   | `LpsType::Float` (F32) or `LpsType::Int` (I32 for `Count`, `Choice`) or `LpsType::Bool`                          |
| `Position2d`   | `LpsType::Vec2`                                                                                                  |
| `Position3d`   | `LpsType::Vec3`                                                                                                  |
| `Color`        | `Struct{ space: I32, coords: Vec3 }` (see `color.md`)                                                            |
| `ColorPalette` | Authoring struct: `Struct{ space: I32, count: I32, entries: Array(Vec3, 16) }`; shader-visible runtime form is a generated height-one `Kind::Texture` resource |
| `Gradient`     | Authoring struct: `Struct{ space: I32, method: I32, count: I32, stops: Array(Struct{at: F32, c: Vec3}, 16) }`; shader-visible runtime form is a generated height-one `Kind::Texture` resource |
| `Texture`      | `Struct{ format: I32, width: I32, height: I32, handle: I32 }` — opaque handle, pixel data lives in `TextureBuffer` |

When `Audio` etc. land:

| Kind         | Storage (sketched, finalized when first example needs it)         |
| ------------ | ----------------------------------------------------------------- |
| `Audio`      | opaque handle (likely; ring-buffered samples)                     |
| `AudioFft`   | `Array(F32, AUDIO_FFT_BINS)` (or opaque if streamed)              |
| `AudioLevel` | `Struct{ low: F32, mid: F32, high: F32 }`                         |
| `Beat`       | `Struct{ bpm: F32, downbeat: Bool, phase: F32 }`                  |
| `Touch`      | `Struct{ count: I32, points: Array(Struct{...}, MAX_TOUCH_POINTS) }` |
| `Motion`     | `Struct{ accel: Vec3, gyro: Vec3 }`                               |

Constants (`MAX_PALETTE_LEN`, `MAX_GRADIENT_STOPS`, `MAX_TOUCH_POINTS`,
`AUDIO_FRAME_LEN`, `AUDIO_FFT_BINS`) live as `pub const` in
`lp-domain` next to the Kinds that use them. v0 values are deliberately
small for embedded targets; bumping them is a single-constant change.

`ColorPalette` and `Gradient` are kept here as authoring/value
recipes. The lpfx MVP materializes them into width-by-one textures
before shader binding, then passes those textures through the same
`sampler2D` / `TextureBindingSpec::HeightOne` path used for other
shader image resources. Schema locking should revisit whether these
remain first-class `Kind`s, become `TextureSpec` recipes under
`Kind::Texture`, or split into separate authoring-only concepts.

## 4. `Dimension` and `Unit`

Dimensions classify what's *commensurable*. Two Kinds share a
Dimension iff their values are convertible.

```rust
pub enum Dimension {
    Dimensionless,  // Amplitude, Ratio, Phase, Count, Bool, Choice
    Time,           // Instant, Duration
    Frequency,      // Frequency
    Angle,          // Angle
}

pub enum Unit {
    None,           // for Dimensionless
    Seconds,        // base for Time
    Hertz,          // base for Frequency
    Radians,        // base for Angle
}
```

### v0 conventions

- **Stored values are in the base unit of their Dimension.** Time
  in seconds, Frequency in Hz, Angle in radians. The TOML schema
  has no `unit` field — it's implied by Kind.
- **`Dimension` is opaque.** No reciprocal or exponent algebra.
  `Time` and `Frequency` are *different* dimensions even though
  they're mathematical reciprocals; conversion is user code
  (`1.0 / param_period` inside a shader), not framework concern.
- **No quantity arithmetic in the framework.** Math happens in user
  shaders, where types don't track.
- **`Dimension::Angle` is a pragmatic convenience.** SI treats
  angle as dimensionless (radian = arc/radius ratio). We keep it
  separate so we can ask "is this rotational?" without inspecting
  Kind.
- **`Phase` is `Dimension::Dimensionless`, not `Angle`.** Phase is
  the normalized form of Angle (`phase = angle / (2π)`). They're
  separate Kinds because of different default ranges and intent;
  conversion is user code.

### Future-additive

- Multi-unit support adds variants to `Unit` and a real
  `to_base_factor()` (`unit = "ms"` → stored as seconds at load).
- Reciprocal/exponent dimensional algebra refactors `Dimension`
  from enum to struct; TOML schema unchanged.

## 5. `Constraint` — what values are legal

```rust
pub enum Constraint {
    Free,
    Range  { min: LpsValue, max: LpsValue, step: Option<LpsValue> },
    Choice { values: Vec<LpsValue>, labels: Vec<String> },
    // ...grows with the example corpus
}
```

Constraints **refine** a Kind's natural domain. `Amplitude`'s
default is `Range[0, 1]`; a Slot can override with a tighter
range, or `Free` for boost-style usage.

Constraints are **domain truth**, not UI hints — a binding that
violates a Slot's Constraint is a compose-time error. UI may
present them (faders snap to step, dropdowns show labels), but the
Constraint itself is about legality.

For color coords specifically, the default is **`Free`** (per
`color.md` — overshoot is meaningful for OOG and boost). A Slot
that wants to enforce in-gamut authoring overrides with `Range`.

## 6. `Shape` and `Slot` — composition

```rust
pub enum Shape {
    Scalar {
        kind: Kind,
        constraint: Constraint,
        default: ValueSpec,                    // mandatory on Scalar
    },
    Array {
        element: Box<Slot>,
        length: u32,
        default: Option<ValueSpec>,            // None ⇒ derive from element
    },
    Struct {
        fields: Vec<(String, Slot)>,           // ordered
        default: Option<ValueSpec>,            // None ⇒ derive from fields
    },
}

pub struct Slot {
    pub shape:       Shape,
    pub label:       Option<String>,
    pub description: Option<String>,
    pub bind:        Option<Binding>,      // §8
    pub present:     Option<Presentation>, // §9; absent = Kind default
}

impl Slot {
    /// What the GPU/serializer sees.
    pub fn storage(&self) -> LpsType;

    /// Single load+bind-time check.
    pub fn validate(&self, v: &LpsValue) -> Result<(), DomainError>;

    /// Materialize the default value. Scalar reads `default` directly;
    /// composed Shapes use the override if present, otherwise derive
    /// from children.
    pub fn default_value(&self, ctx: &mut LoadCtx) -> LpsValue;
}
```

### Why ordered Struct (`Vec<(String, Slot)>`, not `BTreeMap`)

- Matches `lps_shared::LpsType::Struct` (which is ordered for
  std430 layout correctness).
- Preserves authored TOML field order for round-trip.
- Gives Visual panels a deterministic top-to-bottom field order
  for UI generation.

### Why no tuples / sum types in `Shape`

Whatever we model has a direct GPU storage projection. WGSL and
GLSL can express `Scalar`, `Array`, `Struct` natively; not tuples
or tagged unions. Drawing the line here means anything in lp-domain
maps cleanly to a shader uniform / storage buffer.

### Defaults for compositions

A composition's `default` is **literally optional** —
`Shape::Array` and `Shape::Struct` carry
`default: Option<ValueSpec>`. `Scalar` carries a mandatory
`ValueSpec`.

- `default = Some(spec)` ⇒ materialize from `spec` (the only
  way to express aggregate-level overrides like the array
  preset in §10).
- `default = None`        ⇒ derive at load time:
  - `Array`  ⇒ `LpsValue::Array(N copies of element.default_value())`.
  - `Struct` ⇒ `LpsValue::Struct(field_name → field.default_value())`.

Round-trip parity is automatic via
`#[serde(skip_serializing_if = "Option::is_none")]` on the
composed-default fields: a TOML file that omitted `default` on a
struct gets re-saved without one.

## 7. `ValueSpec` — author-time defaults

Some Kinds have defaults that aren't expressible as raw `LpsValue`s:
opaque handles need recipes (color, file path, procedural source).
`ValueSpec` is the abstraction.

```rust
pub enum ValueSpec {
    /// A literal value. Materializes to itself.
    Literal(LpsValue),
    /// A recipe for producing a Texture at materialization time.
    Texture(TextureSpec),
    // Future: Audio(AudioSpec), Video(VideoSpec), ...
}

pub enum TextureSpec {
    /// 1×1 fully-opaque black; the universal "no texture" default.
    Black,
    // Future:
    // Solid { color: [f32; 3] },
    // File { path: String },
    // Procedural { kind: ProceduralKind, ... },
}

impl ValueSpec {
    /// Produces the runtime LpsValue. For Literal, identity. For
    /// opaque-handle variants, allocates resources via the LoadCtx
    /// and returns the handle-form LpsValue.
    pub fn materialize(&self, ctx: &mut LoadCtx) -> LpsValue;
}
```

### Conventions

- **Every Slot can produce a default value at materialize time.**
  `Scalar` Shapes hold a mandatory `default: ValueSpec`. Composed
  Shapes (`Array`, `Struct`) hold `default: Option<ValueSpec>`
  and derive from their children when `None`. See §6 for the
  mechanics. Init-order ambiguity is still closed:
  `Slot::default_value(ctx)` always returns an `LpsValue`.
- **Authored source forms round-trip on save.** We serialize
  `ValueSpec`, not the materialized `LpsValue`. What the user
  wrote is what they get back.
- **Materialization is at load time.** Runtime LpsValue is cached
  by the binding resolver, not stored on the `Slot` (keeps `Slot`
  pure data — friendly to serialization, schemars, tests).
- **Naming.** "ValueSpec" overlaps slightly with `ArtifactSpec` /
  `NodeSpec` / `NodePropSpec` (which are *string identifiers*). The
  overload is bearable; `ValueSpecifier` is the wordier alternative
  if it ever bites us.

## 8. `Binding` — bus connections

```rust
pub enum Binding {
    /// Bind to a named bus channel. The channel's Kind must match
    /// the Slot's Kind exactly (no implicit conversions in v0).
    Bus { channel: String },
    // Future: Const { value: LpsValue },
    //         Modulator { source: NodePropSpec },
    //         Bus { channel, transform: BusTransform },
}
```

### Direction is contextual

A `Binding` doesn't carry direction (read vs. write). A Slot's role
in its container determines that:

- Slots under `[params.*]` → input Slots; the binding says "read
  from this channel."
- Slots under `[output]` (when explicit; mostly implicit in v0
  Visuals) → output Slots; the binding says "write to this channel."

Same `Binding` enum, same TOML form (`bind = { bus = "audio/in/0" }`),
direction implied by container.

### Default bindings live on `Kind`

```rust
impl Kind {
    /// Conventional input binding. Used when an input Slot has no
    /// explicit `bind`. Output Slots resolve their binding through
    /// their containing module (a Show writes to `video/out/0`
    /// because Shows write to `video/out/0`, not because Texture
    /// has a default output binding).
    pub const fn default_bind(self) -> Option<Binding>;
}
```

Examples:
- `Kind::Instant` → `Some(Bus { channel: "time" })`
- `Kind::Texture` → `Some(Bus { channel: "video/in/0" })`
- `Kind::Color`, `Kind::Frequency`, etc. → `None`

### Resolution order at compose time

For each Slot, the effective binding is:

1. **Slot's explicit `bind`** if `Some` — wins absolutely.
2. **`Kind::default_bind()`** if `Some` — convention.
3. **None** — Slot uses its `default` (materialized via `ValueSpec`).

### Compose-time validation

- Bus channel must exist (or be declarable by some module joining
  the project).
- The channel's Kind (set by the first binding to it) must match
  the Slot's Kind exactly. Mismatch is a compose-time error in
  diagnostics:
  ```
  error: bus channel `audio/in/0` has incompatible bindings
    - declared as Kind::Audio at fluid.pattern.toml:[input.audio]
    - declared as Kind::Amplitude at vu.pattern.toml:[input.audio]
  help: rename the channel or reconcile the Kinds
  ```
- No type coercion at the bus.
- No cycle detection in v0; bindings are param ← signal one-way
  (params can't push to channels).

### The bus is implicit

There's no `[bus.channels]` block. The channel set is *derived*
from the bindings declared by Modules joining the Project. A
channel exists iff at least one binding references it.

## 9. `Presentation` — widget choice

```rust
pub enum Presentation {
    Knob, Fader, Toggle, NumberInput, Dropdown, XyPad,
    ColorPicker, PaletteEditor, GradientEditor, TexturePreview,
}
```

10 variants for v0. Defaults via `Kind::default_presentation()`:

| Kind                      | Default Presentation |
| ------------------------- | -------------------- |
| `Instant`, `Count`        | `NumberInput`        |
| `Duration`, `Amplitude`, `Ratio` | `Fader`       |
| `Frequency`, `Angle`, `Phase` | `Knob`           |
| `Bool`                    | `Toggle`             |
| `Choice`                  | `Dropdown`           |
| `Color`                   | `ColorPicker`        |
| `ColorPalette`            | `PaletteEditor`      |
| `Gradient`                | `GradientEditor`     |
| `Position2d`              | `XyPad`              |
| `Position3d`              | `NumberInput` (×3, no XyzPad in v0) |
| `Texture`                 | `TexturePreview`     |

### v0 is enum-only, no per-variant config

Constraints already cover range/step/choices. UI hints (log scale,
format string, wrap behavior) are deferred until concrete examples
demand them — additive when needed (either as enum variants
carrying config, or a parallel `PresentationHints` struct).

### Future-additive

`RadioGroup`, `XyzPad`, `PhaseIndicator`, `ImagePicker`, `FilePath`,
`MultilineText`, custom presentations, panel-level grouping
(basic / advanced sections — those live at the Panel layer, not
here).

## 10. TOML grammar

The TOML form mirrors the recursive `Slot` structure.

### Slot table inference rules

| Trigger                          | Result                                            |
| -------------------------------- | ------------------------------------------------- |
| Top-level `[params]`             | Implicit `Shape::Struct`. Special-cased.          |
| Default (no `shape` field)       | `Shape::Scalar`. `kind` required.                 |
| `shape = "array"`                | `Shape::Array`. `length` + `[X.element]` table.   |
| `shape = "struct"`               | `Shape::Struct`. Fields under `[X.props.<name>]`. |

Reserved field-map keywords: **`params`** (top-level only),
**`element`**, **`props`**. Cannot be used as user-defined Slot
field names.

Constraint fields (`range`, `step`, `choices`, `labels`) live as
peers on a Scalar Slot. `default` may be omitted on compositions
(computed from children); always present on the in-memory Slot.

### Worked example

```toml
# Implicit Struct at the top — no shape declaration needed
[params.speed]
kind    = "frequency"
range   = [0, 5]
step    = 0.1
default = 1.0
label   = "Speed"

# Single color — value carries its space (see color.md)
[params.tint]
kind    = "color"
default = { space = "oklch", coords = [0.7, 0.15, 90] }

# Color palette — shared space, fixed-max storage, explicit count
[params.palette]
kind    = "color_palette"
default = { space = "oklch", entries = [
  [0.7, 0.15,  90],
  [0.6, 0.20,  60],
  [0.5, 0.25,  30],
]}

# Runtime palette/gradient resources are generated textures. The
# authoring value above is what TOML persists; lpfx bakes it into an
# X-by-1 texture and binds it to shaders as a sampler2D.

# Texture input — opaque handle, default = "black"
[input.video]
kind    = "texture"
default = "black"
bind    = { bus = "video/in/0" }

# Array of structs — explicit shape, struct fields under `props`
[params.emitters]
shape   = "array"
length  = 4

[params.emitters.element]
shape   = "struct"

[params.emitters.element.props.position]
kind    = "position2d"
default = [0.5, 0.5]

[params.emitters.element.props.intensity]
kind    = "amplitude"
range   = [0, 1]
default = 1.0

[params.emitters]
default = [
  { position = [0.2, 0.5], intensity = 0.8 },
  { position = [0.5, 0.5], intensity = 1.0 },
  { position = [0.8, 0.5], intensity = 0.6 },
  { position = [0.5, 0.2], intensity = 0.5 },
]
```

### Implementation

Parser is custom `Deserialize` for `Slot`:
- `shape` defaults to `"scalar"` when missing (serde tag default).
- `[params]` top-level is handled by a `ParamsTable` newtype.
- Constraint fields are collected from peers based on the Kind's
  expected constraint shape.
- Default computation for compositions walks child Slots.

~30 lines of custom parser glue beyond `#[derive(Deserialize)]`.
M3 scope.

### Out for v0

- `array = N` shorthand for arrays-of-scalars (deferred).
- Inline TOML shorthand for whole Slots (`x = { kind = "...", default = ... }`).
- Anonymous struct Slots inferred from key shape.
- `oneof` / sum-type Shapes.

## 11. Bus channel naming convention

Channel names follow:

```
<kind>/<dir>/<channel>[/<sub>...]
```

- **`kind`**: lowercase Kind name (`audio`, `video`, `touch`).
  *Convention*, not enforced — the channel's actual Kind is set by
  the first binding.
- **`dir`**: `in` or `out`.
- **`channel`**: zero-based index. **Always present**, even for
  the default. Lets us add sub-channels without retroactively
  shifting names.
- **`sub`** (optional): one or more sub-channel segments for
  derived data (`audio/in/0/bands`, `audio/in/0/level`).

Examples:

| Channel              | Meaning                                                   |
| -------------------- | --------------------------------------------------------- |
| `audio/in/0`         | Primary audio input.                                      |
| `audio/in/1`         | Second audio input (e.g., a USB interface's right ch).    |
| `audio/in/0/bands`   | FFT bands derived from the primary audio input.           |
| `video/in/0`         | Primary camera / video source.                            |
| `video/out/0`        | Primary rendered output (where Shows write).              |
| `touch/in/0`         | Primary touch surface.                                    |
| `time`               | Project clock. Convention exception (no `kind/dir/idx`).  |

This is a **convention** for v0, not a parser rule. May be codified
later if naming chaos becomes a problem; for now it's documentation.

## 12. Non-negotiables

The hard rules. Implementations that violate these are wrong.

1. **Every Slot can produce a default value at materialize
   time.** `Scalar` Shapes carry a mandatory `ValueSpec`.
   Composed Shapes (`Array`, `Struct`) carry an
   `Option<ValueSpec>` override and derive a default from their
   children when `None`. Round-trip preserves whether the
   composed default was explicit. Init-order ambiguity is still
   closed: `Slot::default_value(ctx)` always returns an
   `LpsValue`.
2. **Composition Shapes are `Scalar | Array | Struct`.** No
   tuples, no sum types — anything we model must project to GPU
   storage.
3. **Bus channels are Kind-typed.** First binding sets the Kind;
   subsequent mismatches are compose-time errors. No coercion.
4. **`Shape::Struct` field order is preserved.** Matches GPU
   std430 layout, TOML round-trip, and UI generation order.
5. **Stored values are in their Kind's base unit.** No multi-unit
   in v0; the loader can refuse `unit = "ms"` because the schema
   doesn't have a `unit` field.
6. **Materialization is at load time.** Runtime `LpsValue` is
   cached by the binding resolver, not stored on the Slot. Slot
   stays pure data.
7. **No `Kind::default_bind()` for output Slots.** Output binding
   is a module-level concern (a Show writes to `video/out/0`
   because Shows do; not because Texture has a default output).

See [`color.md`](../color.md) for color-specific non-negotiables
(precision contract, no Unorm8 linear, etc.).

## 13. Future-additive (not in v0)

Designed to grow without breaking the contract:

- **More Kinds** as examples drive them: Audio, AudioFft,
  AudioLevel, Beat, Touch, Motion, MidiNote, OscMessage, ...
- **Multi-unit support.** `Unit::Milliseconds`, etc., with a real
  `to_base_factor()`.
- **More Constraint variants** (`Pattern { regex }`, `MinLen`,
  `MaxLen`, ...).
- **More Presentation variants** (`RadioGroup`, `XyzPad`,
  `PhaseIndicator`, ...) and per-variant hints.
- **More `Binding` variants** (`Const`, `Modulator`, `Bus { ...,
  transform }`).
- **Project-level binding overrides** (e.g., "this device routes
  `audio/in/0` to `audio/in/2`").
- **Explicit output Slots** in TOML (currently mostly implicit).
- **More `ValueSpec` variants** (`Audio`, `Video`, file refs,
  procedural sources).
- **Larger collections.** Bump `MAX_PALETTE_LEN`,
  `MAX_GRADIENT_STOPS`, `MAX_TOUCH_POINTS`. One-constant changes.
- **HDR textures** (`Texture::precision` field).
- **Tone-mapping** at the texture-write boundary.
- **Wider-gamut colorspaces** (Display P3, Rec.2020).
- **Sum-type / oneof Shapes** if a real example demands it.

## 14. Reference implementations

When written, these crates / files implement this contract:

- `lp-domain/lp-domain/src/kind.rs` — `Kind`, `Dimension`, `Unit`.
- `lp-domain/lp-domain/src/constraint.rs` — `Constraint`.
- `lp-domain/lp-domain/src/shape.rs` — `Shape`, `Slot`.
- `lp-domain/lp-domain/src/value_spec.rs` — `ValueSpec`,
  `TextureSpec`, `materialize()`.
- `lp-domain/lp-domain/src/binding.rs` — `Binding`.
- `lp-domain/lp-domain/src/presentation.rs` — `Presentation`.
- `lp-domain/lp-domain/src/loader/` — TOML parser, custom
  `Deserialize` for `Slot`.
- `lp-domain/lp-domain/examples/v1/{kind}/...` — canonical examples
  per Visual kind (M3 corpus + M5 migration baseline).
- `lp-domain/lp-domain/schemas/v1/*.json` — generated schemas,
  immutable post-merge.

## See also

- [`color.md`](../color.md) — Color strategy and precision contract
  (canonical numeric format, color-family Kinds in detail,
  colorspaces, gradient interpolation).
- [`q32.md`](../q32.md) — Q16.16 fixed-point semantics (int-only
  numeric fallback).
- [`domain.md`](./domain.md) — Broader Lightplayer vocabulary
  (Visuals, Modules, Rigs, Bus, Project) that the Quantity model
  serves.
- [`glsl-layout.md`](../glsl-layout.md) — std430 packing rules
  (relevant for Kind storage on the GPU).
