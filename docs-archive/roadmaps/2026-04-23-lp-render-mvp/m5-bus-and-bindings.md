# Milestone 5: Bus + bindings — synthetic editor inputs, binding UI, bus debugger

## Goal

Land the bus and binding machinery: a `Bus` trait in `lp-domain`,
an in-memory `MemBus` impl in lpfx, binding resolution wired
through node instances, editor synthetic-input UI (sliders that
publish to bus channels), a "bind to channel" affordance per
param widget, support for `[input] bus = "..."` on Effect / Stack,
and a bus debugger panel.

After M5, you can: drag an editor slider that publishes to channel
`speed`, bind a Pattern's `speed` param to that channel, watch the
Pattern react. You can also point a Stack's input at `video/in/0`
and have the editor synthesise a test pattern on that channel.
Texture bus values are routed resources: the debugger can display
metadata, but rendering resolves them to backend-owned texture
allocations that can produce `LpsTexture2DValue` uniforms.

## Suggested plan location

`docs/roadmaps/2026-04-23-lp-render-mvp/m5-bus-and-bindings/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

**In scope:**

- **`Bus` trait in `lp-domain/lp-domain/src/bus.rs`**:
  - Paired with the existing `BindingResolver` trait.
  - `read(&self, channel: &ChannelName) -> Option<LpsValue>`.
  - `write(&self, channel: &ChannelName, value: LpsValue)`
    (interior mutability — it's the bus, every consumer
    publishes / reads).
  - `kind_of(&self, channel: &ChannelName) -> Option<Kind>` —
    first writer/reader establishes the Kind per
    `quantity.md` §8.
  - `register_writer(&self, channel: ..., kind: Kind)` /
    `register_reader(...)` for the cascade rules.
  - Trait sized so the future Show layer can implement it
    over a richer topology (multi-source merge, history, etc.)
    without breaking lpfx.
- **`MemBus` impl in `lpfx/lpfx/src/runtime/bus.rs`**:
  - `BTreeMap<ChannelName, BusEntry>` where `BusEntry` is
    `{ kind: Kind, value: LpsValue, last_written_frame: u64 }`.
  - Interior mutability via `RefCell` (single-threaded — wasm)
    or `Mutex` if/when we go multi-threaded.
  - Implements both `Bus` and `BindingResolver`.
  - For `Kind::Texture`, the bus value carries or references a
    stable lpfx texture resource handle; render-time binding
    resolves that handle to the backend-owned buffer and emits a
    `Texture2D` uniform. Do not rely on a bare `{ format, width,
    height, handle }` struct without resolver access to the
    allocation.
- **Binding resolution at instantiation**:
  - When `PatternInstance` instantiates, walks `Pattern.params`
    looking for slots with `bind = { bus = "..." }`. Records
    `(param_name, channel_name)` pairs.
  - At render: for each bound param, read from bus and populate
    the corresponding `params.*` field. Unbound params still use
    in-memory values.
  - Cascade rules from `quantity.md` §8: first reader/writer
    establishes channel Kind; mismatched Kinds error at
    instantiation.
  - `kind = "instant"` default-binds to `time` channel
    (per the `rainbow.pattern.toml` example comment) — implement
    this default.
- **`VisualInput::Bus` support** in `EffectInstance` and
  `StackInstance`:
  - When `[input] bus = "video/in/0"`, the input texture comes
    from a bus channel (Kind::Texture) instead of an upstream
    visual. The resolved texture is bound through the same
    general 2D sampler contract as M4 Effect inputs.
  - Editor synthesises test textures on those channels (next
    bullet).
  - Texture bus values are routed resources: they can populate
    `params.*` texture fields (e.g., `params.gradient`) or graph
    inputs (e.g., `inputColor`).
- **Editor synthetic-input UI** in
  `lp-app/lp-studio/src/components/synthetic_bus.rs`:
  - A panel listing currently-active editor-driven channels.
  - Add a scalar channel: name + initial value + range slider.
    Slider drives `bus.write(channel, value)` on every change.
  - Add a clock channel: publishes `time` (seconds since start)
    every frame. Default-on (most patterns want time).
  - Add a synthetic texture channel: produces a test pattern
    (color bars, gradient, or a small built-in animation) for
    `Kind::Texture` channels that Effects/Stacks consume.
    Synthetic texture channels use resource-texture cache entries
    rather than Stack ping-pong buffers.
  - Channels show their inferred Kind (from `bus.kind_of`) so
    you know what's bound.
- **"Bind to channel" affordance** in widget panel:
  - Each scalar param widget (slider, dropdown, etc.) gets a
    small icon/button that opens a "bind" popover.
  - Popover: list of bus channels of compatible Kind + "create
    new channel" option (creates an editor-synthetic input
    channel of the right Kind).
  - Bound params show as "→ channel-name" in the UI; unbinding
    is one click.
  - When a param is bound, the widget value field shows the
    *current bus value* (read-only display, since the binding
    drives it).
- **Bus debugger page** in
  `lp-app/lp-studio/src/pages/bus_debugger.rs`:
  - Text grid: `(channel, kind, current value, last writer,
    last write frame)`.
  - Refreshes each frame.
  - No history, no plotting (per Q9 resolution — trivial
    debugger only).
- **Routing**: `/bus` route for the debugger, plus an
  always-present "Bus" tab in the top nav.
- **Tests**:
  - Unit: `MemBus` round-trips reads/writes; cascade rules
    error correctly on Kind mismatch.
  - Integration: bind `rainbow.pattern.toml`'s `speed` to a
    bus channel, write a value to the channel, render,
    assert output reflects the value.
  - Default bind for `kind = "instant"` actually wires to the
    `time` channel without explicit `bind = ...` in TOML.
  - `VisualInput::Bus` texture test binds a synthetic texture
    channel, renders an Effect, and proves the sampler uniform
    sees the bus-provided resource.

**Out of scope:**

- Real Show-layer signal generators (LFO, audio, MIDI) — those
  live in lp-engine's future Show roadmap.
- Bus channel plotting / history (debugger stays text-only).
- Multi-source bus merging (Show concern).
- Cross-tab/cross-process bus (only the local lp-studio session).
- Persisting bindings back to TOML files — in-memory only in M5;
  TOML serialization of bindings happens in M6 alongside the
  TOML side-by-side editor.
- Authoring TOML `bind = { bus = "..." }` declarations via UI;
  for M5 you can either edit the file by hand and reopen, or
  use the "bind to channel" affordance which sets the binding
  in-memory.

## Key decisions

- **`Bus` trait lives in `lp-domain`** so future Show-layer impls
  can substitute. The `MemBus` impl lives in lpfx; it's the only
  impl we need for now.
- **`MemBus` is single-threaded** (RefCell-based). lp-studio runs
  in a single wasm thread; lp-engine will eventually need
  thread-safe variants for native runtime, but that's a future
  concern handled by a parallel impl behind the same trait.
- **Cascade rules are enforced at instantiation, not at parse.**
  Same model as the cycle detection in M4. Kind mismatches error
  cleanly with both binding sites in the message.
- **`kind = "instant"` default-binds to `time`** without an
  explicit `bind = ...` in TOML. This is explicit in the
  `rainbow.pattern.toml` comment ("`instant` default-binds to
  'time'; no explicit bind needed"). Implement the default; it's
  a real piece of the lp-domain model that needs runtime support.
- **Editor is a "bus citizen":** it both writes to the bus
  (synthetic inputs) and reads from it (binding-driven widget
  display). This is the pattern that lets the editor be later
  embedded in lp-studio with a real Show-driven bus — same
  contract.
- **Bind-to-channel UI affordance is per-widget**, not a
  separate "bindings page." Bindings belong with the params
  they affect — keeps the cognitive load local.
- **Editor-synthetic textures for `Kind::Texture` bus inputs**
  use built-in test patterns (color bars, etc.). When a real
  Show eventually publishes `video/in/0` from a camera or
  capture card, the same Effect/Stack code consumes it
  unchanged.
- **Texture bus values are handles plus resolver state.** The bus
  establishes `Kind::Texture` compatibility, but the renderer needs
  a live backend allocation to bind a `sampler2D`. Keep metadata for
  display and cascade checks separate from the allocation lookup
  used during render.

## Deliverables

- `lp-domain/lp-domain/src/bus.rs` — `Bus` trait.
- `lpfx/lpfx/src/runtime/bus.rs` — `MemBus` impl.
- Binding resolution wired through `PatternInstance` /
  `EffectInstance` / `StackInstance`.
- `kind = "instant"` default-bind implementation.
- `VisualInput::Bus` support in Effect / Stack runtimes.
- `Kind::Texture` bus channel resolution to live texture resources
  and sampler uniforms.
- `lp-app/lp-studio/src/components/synthetic_bus.rs`.
- "Bind to channel" affordance in widget panel.
- `lp-app/lp-studio/src/pages/bus_debugger.rs`.
- Tests covering `MemBus`, binding resolution, cascade rules,
  default bindings.
- Updated lpfx README + lp-studio README with bus model.

## Acceptance smoke tests

```bash
cargo test -p lp-domain --lib bus
cargo test -p lpfx --test render
# → bind speed to channel, write value, assert output reflects

cd lp-app/lp-studio && dx serve
# → open rainbow.pattern.toml
# → preview shows rolling rainbow at default speed
# → bus debugger shows `time` channel ticking up
# → add synthetic input channel "speed_lfo" with range [0, 5]
# → click "bind" on speed param, pick "speed_lfo"
# → drag the speed_lfo slider, watch rainbow accelerate
# → bus debugger shows speed_lfo value updating
```

## Dependencies

- M4 complete (multi-node graph + Effect / Stack node impls).
- lp-domain `Binding` / `BindingResolver` types already exist.

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: New trait in lp-domain, new impl in lpfx, binding
resolution wiring through three node impl types, default-bind
behaviour for `kind = "instant"`, editor synthetic-input UI,
bind-to-channel widget affordance, bus debugger page. Touches
both the runtime and the editor meaningfully; cascade rules
need careful test coverage. Phaseable: trait + MemBus + binding
resolution + tests as one phase, default-bind + `VisualInput::Bus`
as another, editor synthetic-input UI + bind affordance + debugger
as the third.

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
