# 05 — Slots, Props, namespaces

> **M4.3a update:** Authored slot/source schema moved to
> `lpc-source` (`SrcShape`, `SrcSlot`, `SrcValueSpec`,
> `SrcBinding`). Shared paths/kinds/value shapes remain in
> `lpc-model`. Runtime property reflection is
> `lpc-engine::RuntimePropAccess` over `LpsValueF32`; client-side
> views use `lp-engine-client::WirePropAccess` over `WireValue`.

Two distinct concepts that share a structural type system but live
in different worlds.

- **Slot** — *schema-side*. Lives on the artifact. Declares what
  shape a value has, what kind it is, what its default is, what
  bus channel it would prefer. Authored in TOML.
- **Prop\<T\>** — *runtime-side*. A `(value: T, changed_frame:
  FrameId)` pair; the change-tracking primitive used to publish
  produced values across the wire.

A slot is a declaration; a Prop is a recording. Most node impls
have both: the artifact's slots tell the runtime what to expect,
and the impl's `*Props` struct holds the produced fields it writes
during `tick`.

## `Slot` (lpc-model, shipped in M2)

```rust
pub enum Shape {
    Scalar  { kind: Kind, constraint: Constraint, default: ValueSpec },
    Array   { element: Box<Slot>, length: u32, default: Option<ValueSpec> },
    Struct  { fields: Vec<(NodeName, Slot)>, default: Option<ValueSpec> },
}

pub struct Slot {
    pub shape:       Shape,
    pub label:       Option<String>,
    pub description: Option<String>,
    pub bind:        Option<Binding>,        // author hint about default channel
    pub present:     Option<Presentation>,   // UI widget hint
}
```

Three shapes (`Scalar` / `Array` / `Struct`); no tuples or sum
types — every slot's storage projects cleanly to a `LpsType` and
GPU layouts (`docs/design/lightplayer/quantity.md` §6).

`Slot` carries:

- The structural shape and (mandatory recursively) default.
- Optional human-facing label / description.
- Optional `bind` — *author hint* about the natural bus channel
  for this slot. If a per-instance `NodeConfig` doesn't override
  this slot, resolution falls back to whatever the artifact says
  here, then to `default`.
- Optional `present` — UI widget hint.

`Slot::default_value(ctx) -> LpsValue` materialises the default at
load time (already shipped). Resolution at runtime uses this same
function as the floor.

`Slot::storage() -> LpsType` projects to the GPU / wire layout.

The `Shape` Q-15 decision (M2): `Shape::Scalar` carries a
*mandatory* default, `Shape::Array` and `Shape::Struct` carry
`Option<ValueSpec>` defaults that synthesize from children when
absent. No `Option`-shaped values reach GLSL. Already implemented;
we're documenting, not changing.

## `Slot` lives on the artifact

The artifact defines the slot tree:

- **Visual artifacts:** `Pattern.params: ParamsTable` (which is a
  `Slot` whose `Shape::Struct.fields` are the named params),
  `Effect.params`, `Stack.params`, etc. The artifact author
  controls the schema.
- **Legacy nodes:** their bespoke `*Config` carries the equivalent
  data (e.g., `TextureConfig.width`, `ShaderConfig.glsl_path`).
  M5 *doesn't* port legacy configs into a slot tree — the
  legacy bridge handles them as opaque artifact payloads.
  Conversion to slot trees is a per-node-port project that lands
  when a legacy node migrates to a visual artifact.

The runtime walks the artifact's slot tree once at load time to:

1. Materialise default values (already in `lpc_model::artifact::load`).
2. Build the structural-children list (Stack's `[input]`, etc.).
3. Type-check per-instance overrides against slot kinds.

## The four namespaces

Every node has four distinct slot collections. They have different
authoring rules and different runtime treatment.

```
Node {
    params:   named   slots, consumed (authored)    — bindable
    inputs:   indexed slots, consumed (composition) — Input children, bindable
    outputs:  indexed slots, produced (primary)     — render product
    state:    named   slots, produced (debug)       — recorded, introspectable
}
```

### Consumed vs produced

The crisper split (resolved in [`../notes.md`](../notes.md) §
"Consumed vs produced"):

- **Consumed slots (`params`, `inputs`):** values flow *into* the
  node from outside. Bindable. The node reads them via
  `ctx.resolve(prop_path)` during tick. They have no `Prop<T>`
  field on the impl — values live in the resolver cache on
  `NodeEntry` ([06](06-bindings-and-resolution.md)).
- **Produced slots (`outputs`, `state`):** the node *writes* them
  during tick. Held as `Prop<T>` on the impl's `*Props` struct.
  Read by the wire / editor via `Node::props()`.

| Namespace | Address space     | Consumed/Produced | Bindable?  | Lives where (impl)        |
|-----------|-------------------|-------------------|------------|---------------------------|
| `params`  | named             | consumed          | yes        | resolver cache (entry)    |
| `inputs`  | indexed           | consumed          | yes (incl. Input children) | resolver cache (entry) |
| `outputs` | indexed           | produced          | targetable by `NodeProp` bindings (read-only from outside) | `*Props.outputs` |
| `state`   | named             | produced          | NO (introspectable but not bindable) | `*Props.state` |

`state` is introspectable (the editor reads it via `props()`) but
**not bindable** — exposing it as a binding source promotes
implementation detail to API. If a node wants to publish a piece
of state, it mirrors it to an output (forcing function for
explicit contracts).

### `params` vs `inputs`

Both are consumed and bindable. The split is authoring affordance:

- **`params`** are named, kind-typed, bus-bindable. The Pattern's
  `speed`, the Effect's `amount`, the Fluid's `intensity`. Authors
  bind them to bus channels for live control.
- **`inputs`** are indexed, structural. The Effect's `inputColor`
  texture, the Stack's primary input. Authors connect them to
  child Visuals (which become `Input` children, [01](01-tree.md))
  or to bus textures.

The runtime treats both the same way (resolver cache, pull at
tick); the namespace choice is for authoring clarity and editor
UI.

### Sharing addresses

`PropPath` lives in a single namespace by virtue of its top-level
field:

- `"params.speed"` — into params.
- `"inputs[0]"` — into inputs.
- `"outputs[0].rgb"` — into outputs.
- `"state.compile_time_ms"` — into state.

So the four namespaces are flattened onto a single `PropPath`
syntax with `params` / `inputs` / `outputs` / `state` as
top-level field names. This matches how visual TOML is authored
(`[params.speed]`, `[input]`, `[outputs.0]`, `[state.x]`).

## `Prop<T>` (lpc-model, shipped in M2)

```rust
pub struct Prop<T> {
    value: T,
    changed_frame: FrameId,
}

impl<T> Prop<T> {
    pub fn new(frame_id: FrameId, value: T) -> Self;
    pub fn get(&self) -> &T;
    pub fn get_mut(&mut self) -> &mut T;     // does NOT bump changed_frame
    pub fn set(&mut self, frame_id: FrameId, value: T);
    pub fn mark_updated(&mut self, frame_id: FrameId);
    pub fn changed_frame(&self) -> FrameId;
    pub fn into_value(self) -> T;
}
```

Already shipped. Renamed from `StateField`. Used by the impl's
`*Props` struct to track per-field change frames.

`get_mut` deliberately does *not* bump `changed_frame` — for
slow-mutating fields where the impl wants to inspect-without-dirty
or wants to write a sequence and then `mark_updated` once at the
end. The default ergonomic path is `set(frame, value)`.

## `*Props` structs (the impl's runtime state)

```rust
pub struct TextureProps {
    pub texture: Prop<TextureHandle>,         // state, recorded
    pub frame:   Prop<FrameId>,               // state
    pub output:  Prop<TextureBuffer>,         // outputs[0], produced

    pub time_changed_at: Prop<FrameId>,       // state, recorded
}

pub struct ShaderProps {
    pub program:        Prop<ShaderProgram>,  // state
    pub last_compile:   Prop<FrameId>,        // state
    pub frames_rendered: Prop<u32>,           // state
    pub error:          Prop<Option<String>>, // state
}
```

Convention: `*Props` lives next to the impl in the runtime crate;
fields flat-named (no separate `outputs` / `state` substructs in
the Rust struct). The `PropAccess` derive maps Rust field names to
their namespace (`texture` is `state`; `output` is `outputs[0]`)
via a small per-field attribute.

```rust
#[derive(PropAccess)]
pub struct TextureProps {
    #[prop(state, name = "texture")]   pub texture: Prop<TextureHandle>,
    #[prop(state)]                     pub frame:   Prop<FrameId>,
    #[prop(outputs, idx = 0)]          pub output:  Prop<TextureBuffer>,
    #[prop(state)]                     pub time_changed_at: Prop<FrameId>,
}
```

The exact derive shape is settled in M4 / M5 as the trait is
implemented. The decision here is the *separation* of consumed
(in resolver cache) vs produced (`*Props.field: Prop<T>`).

## `PropAccess` (object-safe reflection)

Returned by `Node::props()`. Used by the sync layer to walk
produced values.

```rust
pub trait PropAccess {
    fn get(&self, path: &PropPath) -> Option<LpsValue>;

    /// Iterate produced fields whose `changed_frame > since`.
    /// Caller filters by namespace if needed.
    fn iter_changed_since(
        &self, since: FrameId,
    ) -> Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + '_>;

    /// All produced fields' current values + frames. Used by the
    /// snapshot path on first connect / detail-request.
    fn snapshot(&self)
       -> Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + '_>;
}
```

- **`get` is `LpsValue`-typed**, not `T`. Consumers see structural
  values; the impl's typed fields are an internal optimisation.
- **`iter_changed_since` is the diff source** for the wire
  `state_ver`-driven sync ([07](07-sync.md)).
- **`snapshot` is the cold-start path.** A new client connects;
  the sync layer asks every node for its full prop dump.

The trait is *only* called on `Alive` entries. Pending and Failed
entries report empty snapshots — their values are the artifact
defaults, which the editor can compute itself from
`Slot::default_value`.

## Resolver cache (consumed slots)

Living on `NodeEntry`:

```rust
pub struct ResolvedSlot {
    pub value: LpsValue,
    pub changed_frame: FrameId,        // for ctx.changed_since
    pub source: ResolveSource,         // Override / ArtifactBind / Default
}

pub enum ResolveSource {
    Override(BindingKind),     // came from NodeConfig.overrides
    ArtifactBind(BindingKind), // came from Slot.bind on the artifact
    Default,                   // fell through to Slot.default
    Failed,                    // resolver couldn't satisfy; using default-floor
}
```

`resolver_cache: BTreeMap<PropPath, ResolvedSlot>` on the entry.
Populated lazily on first `ctx.resolve(prop)` call per tick;
invalidated when any of:

- `entry.config_ver` increases (override edit, hot reload, set_property).
- The `ArtifactRef`'s `content_frame` changes (artifact reload).
- For `NodeProp` bindings: the targeted node's `*Props.outputs[N]`
  Prop's `changed_frame` increases.
- For `Bus` bindings: the channel's writer's `changed_frame`
  increases.

Cache lookup is O(log n) per slot; tick-time access cost is bounded
by the number of slots an impl actually touches. The cache is the
source-of-truth for `ctx.changed_since` — no extra bookkeeping.

## Why split consumed and produced this way?

Three reasons. Pin them here so future redesigns understand the
constraint stack.

1. **Authoring makes them feel different, but the runtime model
   matches.** Authors think of `params` as inputs; the runtime
   model says "produced values flow over the wire as deltas;
   consumed values are pulled fresh from the binding stack at
   tick." The split honours both views.
2. **Wire diff cost.** Sync only walks produced fields
   (`*Props.iter_changed_since`). If consumed values were also
   `Prop<T>` on the impl, the diff would need to know which to
   skip — adding a field-level "is this consumed?" flag and
   walking past it. The cleaner split: produced lives where it's
   diffed; consumed lives where it's resolved.
3. **Avoids one-frame-stale params.** If params were `Prop<T>` set
   from a `update_config` hook, then a tick that runs *before*
   the hook would see stale values. Pull-at-tick is always fresh.

## Slot validation

Cross-cutting validation that touches both schema and wire:

- **Override Kind matches Slot Kind.** A `Bus` channel's eventual
  `Kind` (when audio writes `audio/in/0/level: AudioLevel`) must
  match the slot's Kind. Mismatch → resolution warns, defaults.
  Compose-time validation in M5 (lazy at first-use); compile-time
  validation in lp-vis when binding-resolver lands.
- **`Slot.bind` Kind matches Slot Kind.** Caught at artifact load
  if we're strict; at resolution if we're lenient. M5: lenient,
  warn at resolution.
- **Override `Binding::Literal`'s `WireValue` / `SrcValueSpec` shape matches
  `Shape.storage()` / `WireType`.** A scalar slot can't accept a struct literal.
  Caught at config-load by existing value-spec helpers
  (`from_toml_for_kind`; naming migrates with M4.3a).

## Where this code lives

- **`lpc-source/src/prop/`** (after M4.3a) — authored `SrcSlot` /
  `SrcShape`, `SrcBinding`; foundation `Kind` / constraints may remain in
  `lpc-model` per split notes.
- **`lpc-engine`** — `Prop<T>` (runtime introspection surfaces through
  `RuntimePropAccess`); **`lpc-engine/src/resolver/`** — resolver cache and
  binding stack (targets M4.3 spine).
- **`lp-legacy/lpl-model/...`** — legacy `NodeProps` trait (retires M5).
- **Derived reflection** — `RuntimePropAccess` + optional `lpc-derive`
  crate (or temporary home in `lpc-engine`).

## Open questions

- **Q-C — `NodePath` / `PropPath` extension grammar.** Cross-tree
  paths (`%unique-name`, `..`, `.`, project-root prefix) are still
  unbound. M5 uses absolute paths only; relative addressing comes
  with the binding-resolver in lp-vis. Flagged.
- **`PropAccess` `get` for nested struct paths.** `outputs[0].rgb`
  on a `Vec3` output: the derive macro needs to either flatten or
  recurse. Implementation detail; pin in M4.
- **Texture metadata on wire.** The legacy `TextureBuffer` is opaque
  bytes in-process (`LpsValueF32`). On the wire, ship **`WireValue::Texture`**
  metadata only; the client's `Prop` mirror never holds GPU bytes. Lossy by
  design; thumbnails use a separate channel. Pin detail in M4 / M4.4 sync.
