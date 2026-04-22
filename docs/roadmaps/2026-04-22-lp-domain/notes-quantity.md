# Quantity model — design notes (historical)

> **The canonical Quantity model lives in
> [`docs/design/lightplayer/quantity.md`](../../design/lightplayer/quantity.md).**
> Color-specific contract: [`docs/design/color.md`](../../design/color.md).
> Q16.16 fallback: [`docs/design/q32.md`](../../design/q32.md).
>
> This file is preserved as a historical record of the Q&A
> sessions that produced those documents. Implementations should
> read the design docs, not this file. Decisions here may have been
> superseded by later refinements.

The conversation that produced the Quantity model worked through
nine "open questions before implementation" (Q1-Q9). Each is
recorded below with its resolution, in roughly the order it was
resolved. They serve as a record of *why* the design landed where
it did.

## Q1 — Permanent home for the Quantity model — RESOLVED

`color.md` got hoisted to `docs/design/color.md` as the canonical
spec. The broader Quantity model (Slot/Shape/Kind/Dimension/Unit/
Constraint, conventions, precision contract pointer) initially
lived only in this roadmap working doc, which will eventually get
archived. M2 implements exactly these types and needs an
authoritative reference.

**Decision:** Hoist the canonical Quantity model to
`docs/design/lightplayer/quantity.md` (joining `domain.md` and
`notes.md` in the same directory). Same structure as `color.md`:
TL;DR, why-it-matters, type sketch, conventions, precision-contract
pointer, non-negotiables, future-additive escapes. Promote the TOML
worked example with it (M3's parser needs authoritative examples;
roadmap notes shouldn't be load-bearing). Trim `notes-quantity.md`
to a pointer + the open-questions list. Doc lives under
`lightplayer/` (not `lp-domain/`) so it survives crate renames —
the *domain* is stable, the crate name is not.

## Q2 — `Binding` enum shape and TOML form — RESOLVED

`Slot.bind` referenced but undefined. Outputs also bind to the bus
(a Show writes to `video/out/0`), so the type can't be source-only.

**Decision:** Rename to `Binding` (drop "Source"). Single v0 variant:
`Bus { channel: String }`. Same enum used for both input and output
Slots — direction comes from the Slot's role in its container (under
`[params.*]` vs an output declaration), not from the `Binding`
type. TOML form: `bind = { bus = "time" }`. Default bindings live on
`Kind` via `Kind::default_bind() -> Option<Binding>` and are
**input-side only** in v0 (output defaults are a module-level
concern — Shows write to `video/out/0` because Shows write to
`video/out/0`). Resolution order at compose time: explicit `Slot.bind`
→ `Kind::default_bind()` → use `Slot.default`. Strict shape-match
validation, no implicit conversions in v0. Future-additive: `Const`,
`Modulator`, `Bus { channel, transform }`, project-level rebinding
tables, explicit output Slots.

## Q3 — `Presentation` enum variant set — RESOLVED

`Slot.present: Option<Presentation>` referenced but undefined.

**Decision:** Enum-only (no per-variant config in v0), 10 variants:
`Knob`, `Fader`, `Toggle`, `NumberInput`, `Dropdown`, `XyPad`,
`ColorPicker`, `PaletteEditor`, `GradientEditor`, `TexturePreview`.
Defaults via `Kind::default_presentation()`. TOML form is a string
discriminator (`present = "fader"`); absent = use Kind default.
Constraint already covers range/step/choices; UI hints (log scale,
format string, wrap) deferred until the first concrete example
demands them — additive when needed (either as enum variants
carrying config, or a parallel `PresentationHints` struct).
Future-additive: `RadioGroup`, `XyzPad`, `PhaseIndicator`,
`ImagePicker`, `FilePath`, `MultilineText`, custom presentations,
panel-level grouping.

**Side decision (Kind list cleanup):**

  - **Rename `Time` → `Instant`.** "Time" is overloaded in everyday
    language (moment vs. span). `Instant` follows Rust's
    `std::time::Instant`/`Duration` convention; precise and
    unambiguous. Bus channel name stays `time` (shader/art
    convention everyone knows). TOML: `kind = "instant"`.
  - **Add `Phase` Kind.** Cyclic [0, 1) F32. Useful for LFO outputs,
    beat phase, animation progress. Distinct from `Ratio` (a
    non-wrapping fraction/proportion) by *intent*; mathematically a
    normalized `Angle` (`phase = angle / (2π)`).
    `Dimension::Dimensionless`, `Unit::None`, `Constraint::Free`
    (wrap is implicit in the Kind, not a Constraint), default
    presentation `Knob` (a `PhaseIndicator` widget can land later).
    No default bind. Phase and Angle stay as separate Kinds in v0
    even though SI treats both as dimensionless — keeping
    `Dimension::Angle` is a pragmatic v0 convenience for asking
    "is this rotational?". Conversion between Phase and Angle is
    user code (same pattern as Time↔Frequency).

## Q4 — `lp-domain` ↔ `lps-shared` crate-graph direction — RESOLVED

The notes initially said `lp-domain`'s `LpsValue` "extends what
lps-shared already has." But who depends on whom?

**Decision:** `lp-domain → lps-shared`. lp-domain depends on
lps-shared and re-exports `LpsType` / `LpsValueF32` (as `LpsValue`)
directly without redefining. lps-shared remains the GPU-truth
foundational layer (`no_std + alloc`); lp-domain layers semantic
meaning (Kind, Constraint, Slot, Binding, Presentation) on top.

**Side decisions:**

  - **F32-only canonical for now; Q32 deferred.** Rather than
    feature-flagging F32/Q32 in lp-domain from day one, use F32
    everywhere and try soft-floats on int-only firmware. Benchmark
    will tell us if a Q32 specialization is needed.
  - **`Shape::Struct` is ordered, not `BTreeMap`.** Matches
    `lps-shared`'s `LpsType::Struct` (GPU std430 layout depends on
    member order anyway), preserves authored TOML field order for
    round-trip, and gives Visual panels a deterministic
    top-to-bottom field order for UI. Use `Vec<(String, Slot)>`.
  - **Texture Kind storage** uses `lps_shared::TextureStorageFormat`
    / `TextureBuffer` (resolves part of Q8).

## Q5 — Test corpus for M5 migrations — RESOLVED

M5 (migration framework) specifies "canonical examples per version"
as the migration test corpus, but where do they come from?

**Decision:** M3's example corpus *is* the v1 baseline.

  - **M3** lays down `lp-domain/lp-domain/examples/v1/{kind}/...`,
    canonical examples per Visual kind covering the parser surface.
  - **M4** generates `lp-domain/lp-domain/schemas/v1/{kind}.schema.json`
    plus a `latest/` mirror; CI fails on regeneration drift, and
    once-merged versioned schemas are immutable (CI-enforced).
  - **M5** ships the migration framework (`Migration` trait,
    registry, CLI) **plus a synthetic `v0_5` → `v1` smoke test**
    designed to exercise the framework end-to-end (field rename,
    struct reshape, default change, optional→required, array length
    change). The smoke test runs in CI and remains as the
    framework's regression guard.

## Q6 — schemars + recursive `Shape`/`Slot` — RESOLVED

M4 (schema generation) assumes `schemars` round-trips our recursive
types cleanly.

**Decision:** Validate **incrementally during M2**, not as a
pre-M2 spike. Add `schemars` to lp-domain dependencies from day
one; every new public type gets `#[derive(JsonSchema)]` alongside
serde. Issues surface immediately as types land. Add an optional
`schemars` feature to `lps-shared` so its `LpsType` participates in
derived schemas (one optional derive, int-only firmware doesn't pay
for it). M4 then becomes pure tooling work (CLI dump, CI drift
gate, immutability check). Skip explicit `jsonschema::validate`
tests in v0; serde deserialization of M3 example artifacts is
sufficient schema-conformance validation. Fallback chain if
schemars chokes on something specific: manual `JsonSchema` impl →
hand-written schema file → alternative generator → drop schema
generation.

## Q7 — TOML sugar for `Slot` shape inference — RESOLVED

**Decision:** Hybrid grammar with `shape` defaulting to `scalar`.

| Trigger                          | Result                                            |
| -------------------------------- | ------------------------------------------------- |
| Top-level `[params]`             | Implicit `Shape::Struct`. Special-cased.          |
| Default (no `shape` field)       | `Shape::Scalar`. `kind` required.                 |
| `shape = "array"`                | `Shape::Array`. `length` + `[X.element]` required.|
| `shape = "struct"`               | `Shape::Struct`. Fields under `[X.props.<name>]`. |

Reserved field-map keywords: `params` (top-level only), `element`,
`props`. Cannot be used as user-defined Slot field names.
Constraint fields (`range`, `step`, `choices`, `labels`) live as
peers on a Scalar Slot. `default` optional on compositions
(computed from children); always present on the in-memory Slot.

Implementation: serde tagged enum on `shape` with a small custom
layer to default the missing tag to `"scalar"` and to handle the
implicit-`[params]` top-level. ~30 lines; M3 scope.

Out for v0: the `array = N` shorthand for arrays-of-scalars
(deferred until repetition becomes painful in real examples),
inline TOML shorthand for whole Slots, anonymous struct Slots,
sum-type Shapes.

## Q8 — `Texture` Kind storage recipe — RESOLVED

`Color`/`ColorPalette`/`Gradient` storage is locked. `Texture` was
referenced but its storage was undefined.

**Decision:** Opaque-handle storage. `Texture`'s `LpsType` is
`Struct { format: I32, width: I32, height: I32, handle: I32 }`
where `format` is `lps_shared::TextureStorageFormat as i32` and
`handle` references a GPU-resident `lps_shared::TextureBuffer`
(handle = 0 means unbound). Default v0 format is Unorm16 linear.
Default presentation `TexturePreview`. Default bind
`Some(Bus { channel: "video/in/0" })` (input-side; output Slots
ignore).

**Side decision (broader Quantity model change):** `Slot.default`
changes from `LpsValue` to `ValueSpec` to handle opaque-handle
Kinds whose author-time defaults aren't expressible as raw byte
data:

```rust
pub enum ValueSpec {
    Literal(LpsValue),
    Texture(TextureSpec),
    // future: Audio(AudioSpec), Video(VideoSpec), ...
}

pub enum TextureSpec {
    Black,
    // future: Solid { color }, File { path }, Procedural { ... }
}
```

TOML for Texture defaults: `default = "black"`. Author intent
round-trips on save (we serialize `ValueSpec`, not the materialized
`LpsValue`). Loader materializes at load time; runtime cache lives
in the binding resolver, not on the Slot — keeps Slot pure data.

The "every Slot has a default" v0 convention updates to:

> Every Slot has a `default: ValueSpec`. For value-Kinds, `default`
> is `ValueSpec::Literal(LpsValue)`. For opaque-handle Kinds
> (Texture, future Audio/Video), `default` is the corresponding
> source variant. The loader materializes sources to runtime
> `LpsValue`s at load time. Authored source forms round-trip on save.

## Q9 — Signal types in the Kind layer — RESOLVED

`domain.md` lists `Video`, `Audio`, `AudioFFT`, `AudioLevel`,
`Beat`, `Touch`, `Motion` as Signal types. Where do they live?

**Decision:** No separate `SignalType` / signal-Shape layer. Each
signal type is a **`Kind`**.

  - **Video → `Texture`** (collapses; video is just a Texture that
    changes over time, sampling it gets you the latest frame).
  - **`Audio`**, **`AudioFft`**, **`AudioLevel`**, **`Beat`**,
    **`Touch`**, **`Motion`** each become Kinds with their own
    storage recipes. Project-wide constants (`AUDIO_FRAME_LEN`,
    `AUDIO_FFT_BINS`, `MAX_TOUCH_POINTS`, ...) live as `pub const`
    in lp-domain alongside `MAX_PALETTE_LEN` / `MAX_GRADIENT_STOPS`.
  - Streamed/bulk Kinds (likely `Audio`, possibly `Touch`) use
    opaque-handle storage same as `Texture`. `ValueSpec` extends
    with their source variants when added (`Audio(AudioSpec)`,
    etc.) — same pattern as `TextureSpec`.

**Bus channel typing:**

  - The bus is implicit (no `[bus.channels]` block in TOML).
  - A channel is a name string (`audio/in/0`, `video/out/0`).
  - The channel's Kind is **derived from the first binding**.
  - All other bindings to the same channel must declare the same
    Kind. Mismatch is a compose-time error (load + bind diagnostic),
    not runtime.
  - No type coercion at the bus.
  - Direction (`in`/`out`) is in the channel name by convention,
    not enforced by Kind.

**Bus channel naming convention:** `<kind>/<dir>/<channel>[/<sub>...]`
— always include the channel index even for the default
(`audio/in/0`, not `audio/in`). Lets us add sub-channels
(`audio/in/0/bands`) without retroactively shifting names.
Convention only for v0; may be codified later.

What this saves: an entire concept layer (`SignalType` /
`signal::*` module / `Shape::matches` machinery / `bus.channels`
TOML grammar). What we don't lose: every signal is expressible,
new signals are additive (one new Kind variant), type-safety at
bind time is enforced by Kind equality.

Out for v0: the audio/beat/touch/motion Kinds themselves — they
land in M3 (or later) when the first example Visual that needs
them appears. Texture lands in M2/M3.
