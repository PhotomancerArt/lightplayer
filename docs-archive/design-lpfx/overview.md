# lpfx overview

The Visual layer of Lightplayer. See [`../lightplayer/domain.md`](../lightplayer/domain.md) for
the broader domain model and [`../lightplayer/quantity.md`](../lightplayer/quantity.md) for
the Quantity vocabulary (kinds, slots, TOML grammar).

## Schema baseline

The canonical v1 example corpus lives at
[`lp-domain/lp-domain/examples/v1/`](../../../lp-domain/lp-domain/examples/v1/),
exercising all six Visual kinds (Pattern, Effect, Transition,
Stack, Live, Playlist). The TOML grammar that drives the
`[params]` section is locked in [`quantity.md` §10 — TOML grammar](../lightplayer/quantity.md#10-toml-grammar).
Each example sets `schema_version = 1` as the first field; M5
introduces the migration framework that will let v2 examples
live alongside v1 in `examples/v1/<kind>/history/`.

## Conventions (decided so far)

- **Param section**: every Visual declares its parameters under `[params]`.
  Both user-tunable knobs and engine-driven values (`time`, `progress`) are
  declared here. There is no separate `[config]` section — builtins put
  their compile-time knobs in `[params]` too, and may flag them internally
  as static.
- **Shader source** is declared in a single `[shader]` table
  with exactly one of `glsl = "..."` (inline source),
  `file = "main.glsl"` (sibling file; language inferred from
  extension), or `builtin = "fluid"` (built-in Rust impl). The
  former `[builtin]` block is gone in v1.
- **GLSL uniform naming**: structural uniforms unprefixed (`outputSize`,
  `input`, `inputA`, `inputB`); user/engine params prefixed `param_`
  (`param_time`, `param_speed`, `param_progress`).
- **Param kinds** — every param declares
  `kind = "<snake_case>"` from the open `Kind` enum
  (`amplitude`, `ratio`, `phase`, `count`, `color`, `audio_level`,
  ...). The Kind picks the storage type, default
  presentation, default constraint, and default bind. See
  [`quantity.md` §3 — `Kind`](../lightplayer/quantity.md#3-kind--the-semantic-identity-layer).
- **Presentation** is derived from the param's
  `Kind` (`Kind::default_presentation()`) and may be overridden
  per-param with `presentation = "<variant>"`. Pre-v1 per-param
  widget sub-tables under a `ui` key (fader, stepper, color picker,
  select, checkbox) are not used in the v1 on-disk form.
- **`label`**, **unit** — `label` is a direct field on each param
  entry under `[params]`. `unit` is **gone** in v1: stored values are
  always in the `Kind`'s base unit (radians for Angle, Hz for
  Frequency, etc.) per [`quantity.md` §4 — Dimension and Unit](../lightplayer/quantity.md#4-dimension-and-unit).
- **`time` / `progress`**: declared explicitly as `[params.time]` /
  `[params.progress]`. Default-bound to bus channels `time` and the
  parent Show's progress signal respectively.
- **Bus channel names**: `<type>/<dir>` for the default/single channel,
  `<type>/<dir>/<n>` when there's more than one. Examples: `video/in`,
  `video/in/1`, `audio/in`, `time`.
- **Transition section**: always `[transition]`. Live and
  Playlist both declare a single playlist-wide `[transition]`.
  Per-entry transition overrides are deferred (see M3 plan
  notes).

## Bindings

A binding routes a value into a `param` (or `input` / `output`) of a Node.
The value comes from one of three **source kinds** (a Rust enum at the
core):

- **`bus`** — a bus channel (`time`, `audio/in`, `video/in/1`)
- **`node`** — another Node's property (`audio.lfo#out`, `/main.show/x#y`)
- **`value`** — a literal constant (`42`, `[1.0, 0.5, 0.5]`)

### Declaration form: explicit table

Always a table with exactly one source-kind key (`bus` / `node` / `value`).
Easier to parse, easier to validate, easier to add shorthand later than
to take it away.

```toml
bind = { bus   = "time" }
bind = { bus   = "audio/in" }
bind = { node  = "audio.lfo#out" }              # relative
bind = { node  = "/main.show/audio.lfo#out" }   # absolute
bind = { value = 42 }
```

Transforms (scale, offset, smoothing, curve) — TBD. They'll attach to
the table form when we get there.

### Where bindings are declared

**1. Inline default on the declaration.** A Visual ships a sensible
default with its `param` / `input` / `output` declaration:

```toml
[params.time]
kind    = "instant"
default = 0.0
# default-binds to bus `time`; add `bind = { bus = "…" }` to override
```

**2. `[bindings]` block on any composite node.** Any node that contains
other nodes (Stack, Live, Playlist, Show, Rig, Project) can override
descendant bindings:

```toml
# inside setlist.playlist.toml
[bindings]
"entries/0#time"      = { bus  = "audio/beat-clock" }   # override fluid's default
"entries/0#emitter_x" = { bus  = "touch/in.x" }
"entries/2#slices"    = { node = "midi.controller#cc14" }
```

Keys are `NodePropSpec`s **relative to the current node**. Values use
the same source forms as inline `bind`.

### Resolution rule: ancestor wins

When the same target has bindings at multiple levels, the **furthest-up
declaration wins**. Reasoning: the project has the most deployment
context (knows the actual rig, the available channels); the pattern only
knows generic conventions.

So a pattern's `bind = { bus = "time" }` is only effective if no
enclosing node overrides it. This makes patterns reusable: ship sensible
defaults; let the deployer reroute.

### Path syntax in `[bindings]` keys

- `effects/0#slices` — relative to current node, indexed child
- `fluid.pattern#emitter_x` — relative to current node, named child
- `/main.show/fluid.pattern#x` — absolute from project root

No `..` (no upward references). To reach outside your subtree, declare
the binding from a higher level.

### Open

- Transform pipeline shape — flat table fields, ordered list, or chained nodes?
- Auto-route by type — does the engine implicitly bind `param_time` to
  bus `time` when no explicit binding exists, or must every time-using
  visual declare `bind = { bus = "time" }`?
- Shorthand sugar — once the explicit form is bedded in, do we add bare
  string for the bus case (`bind = "time"`)? Or stay strict?
- Outputs: do `output` declarations need `bind` too (for "this Visual
  publishes its output texture as `video/out/foo`")? Probably yes.

## Open questions

### Priority computation for Live shows

Live shows pick the highest-priority candidate each frame. How does
priority get computed?

A Visual's priority typically depends on its inputs (e.g., the
audio-fluid candidate is "active" when audio level is high). Today
our Visual outputs are textures only — we don't have a clean way for
a Visual to expose a non-visual scalar like "how active am I?"

Likely answer: per-Visual priority shader (or builtin) that runs each
frame and writes a `param_priority` Property the Live show reads. But
this is the first non-visual output we'd have, and the model isn't
obvious yet.

Not blocking — Live shows aren't shipping in v0. Revisit when we get
to Live show implementation.
