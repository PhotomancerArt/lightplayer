# 02 — `Node` trait

`Node` is the small object-safe runtime spine that every node-type
implements. It lives in `lpc-engine`. Property reflection
(`RuntimePropAccess`) is exposed via a method, not a supertrait.

## Trait surface

```rust
pub trait Node: Send + Sync + 'static {
    /// REQUIRED. Advance one engine step. The single "do work" hook;
    /// reads current params/inputs/artifact via ctx, writes
    /// outputs/state into self.
    fn tick(&mut self, ctx: &mut TickContext)
        -> Result<(), NodeError>;

    /// REQUIRED. Tear down: release engine-side resources (channels,
    /// buffers, JIT memory). Cannot fail meaningfully — log on error.
    fn destroy(&mut self, ctx: &mut DestroyCtx)
        -> Result<(), NodeError>;

    /// REQUIRED, no default. Memory-pressure response: shed what's
    /// reconstructable, keep essential state. Distinct from the
    /// engine-driven Alive → Pending demotion (which calls destroy).
    /// If your node has nothing to shed, that's a deliberate `Ok(())`
    /// — say so in a comment.
    fn handle_memory_pressure(
        &mut self,
        level: PressureLevel,
        ctx: &mut MemPressureCtx,
    ) -> Result<(), NodeError>;

    /// REQUIRED. Generic property reflection for editor + sync layer.
    /// Backed by the impl's typed `*Props` struct (outputs + state
    /// only — params/inputs values live in the resolver cache, see §05).
    fn props(&self) -> &dyn PropAccess;
}
```

Four methods. All required, no defaults.

The trait carries **no tree links**. `Node` impls do not know their
own `NodeId`, parent, or children — those live on the `NodeEntry` /
`NodeTree` (single source of truth, [01](01-tree.md)). Anything a
hook needs from the tree comes through the `*Context` argument.

## Decisions

- **One trait, no `NodeProperties` supertrait.** `PropAccess`
  (returned by `props()`) does the property-reflection job M2's
  `NodeProperties` had. The editor doesn't trait-object live nodes
  — it holds `NodeView` snapshots populated by the sync layer
  ([07](07-sync.md)) — so a separate property-only supertrait
  isn't earning its keep.
- **No `uid()` / `path()` accessors.** The tree is the source of
  truth for identity. Contexts (`TickContext` etc.) carry the
  `NodeId` when an impl needs it for log lines.
- **Construction in `D::instantiate`, not `Node::init`.** The
  `EntryState::Pending → Alive` wake calls
  `D::instantiate(artifact, config, ctx)` to produce a
  `Box<dyn Node>`. No two-step "is it initialised yet?" state —
  once an entry is `Alive`, the node is fully constructed
  ([08](08-domain.md)).
- **No `update_config` / `update_artifact` methods.** Both retired
  in favour of pull-at-tick:
  - `ctx.resolve(prop_path)` returns the current value of any
    `params` / `inputs` slot, walking the binding stack
    ([06](06-bindings-and-resolution.md)).
  - `ctx.changed_since(prop_path, frame)` says whether that slot
    changed since the given frame — the cheap "should I re-do work?"
    check, backed by the resolver cache.
  - `ctx.artifact()` returns the current artifact ref;
    `ctx.artifact_changed_since(frame)` is the artifact analogue.
  This keeps `tick` as the single reconciliation point and removes
  "did you remember to react in `update_config`?" risk.
- **No event channel in M5.** No `NodeEvent` enum, no `handle_event`
  method. Artifact reload is a pull at tick; missing artifact is
  engine-handled (entry → `Failed`, no node hook). See §1.Y below.
- **`handle_memory_pressure` has no default impl.** Forcing function:
  every node-type author must explicitly write at least `Ok(())` and
  look at the doc comment that explains the contract. An event
  variant or default-no-op would be too easy to ignore on embedded.
- **`shed_optional_buffers` renamed `handle_memory_pressure`.** Same
  role, clearer name. Distinct from the engine-driven `Alive →
  Pending` demotion ([01](01-tree.md)), which calls `destroy` on
  the whole node; `handle_memory_pressure` keeps the node alive but
  asks it to release reconstructable buffers.
- **`as_any` / `as_any_mut` move off `Node`** to a separate
  `Downcastable` trait used only by the M5 legacy bridge. Not part
  of the spine surface.
- **Bottom-up wake ordering** per Godot's `_propagate_ready`
  (prior-art §1). Lazy children are woken bottom-up: the descendants
  of a node `Pending → Alive` first, then the node itself.
  `D::instantiate` for a parent may assume all of its descendants
  are alive.
- **Children-first `destroy` ordering.** Parents observe a clean
  teardown after their subtree is gone. (See §1.Y for the "we may
  want a top-down `pre_destroy` later" note — flagged as the most
  plausible reason to introduce events.)
- **`tick` is opt-in via the tree's render schedule, not universal.**
  Per prior-art §1 ("avoid universal per-tick callbacks"). Nodes
  that produce something visible to the render path are visited;
  pure passive nodes are not. The decision is structural (driven
  by the tree's render schedule), not a per-node flag.
- **Panic isolation around `tick` and `handle_memory_pressure`.**
  `destroy` is *not* wrapped — a panic during destroy is a real
  bug with no recovery path other than process exit. Wrapped hooks
  go through `panic_node::catch_node_panic` (existing crate) when
  the `panic-recovery` feature is on.
- **No async at this layer.** Embedded targets are single-thread;
  the client / server async story lives in `lp-server` /
  `lp-client`.

## §1.X — `tick` and time

`tick` replaces `render` from the legacy `NodeRuntime` trait. The
rename is not cosmetic; it reflects two design positions.

**Position 1 — visual / non-visual is unified.** `render` carries a
"draws pixels" connotation that doesn't fit half the planned node
catalogue (Bus, Modulator, Fixture, Output, MIDI). `tick` is the
neutral name for "advance this node by one engine step."

**Position 2 — Lightplayer is not a real-time engine, and time is
not a fundamental concept of the spine.** Mainstream engines
(Godot, Unity, Bevy) take `delta` because they assume game-loop
semantics. Lightplayer is closer to a modular synth: a graph
evaluated per output frame, where "time" is one signal among many
on the bus, not a property of the engine.

Concretely:

- **`tick` takes no time argument.** Signature is
  `fn tick(&mut self, ctx: &mut TickContext) -> Result<(), NodeError>`.
- **Time is a bus signal**, not a tick parameter. The engine
  publishes a `time` channel on the bus (e.g.,
  `engine/time_secs: f64`, `engine/delta_secs: f32`) — a *default
  convention*, not a hardcoded contract. A node that needs it
  (fluid sim, integrators, time-dependent oscillators) takes a
  param bound to `engine/delta_secs`.
- **This makes time composable.** Bind `delta_secs` to an LFO and
  the fluid sim slows down. Bind it to zero and it freezes. Bind
  it to a recorded automation track and you get scrubbing for free.
  None of this requires special engine support.
- **Per-node time is a node-level decision.** Most nodes (Pattern,
  Stack, Effect, Texture) don't need time. The few that do
  (Fluid, Integrator) declare a `delta_secs: f32` param and bind it.
- **Real-time scheduling lives outside the spine.** "Render at 60
  FPS" is a `lp-server` policy: it advances the engine's `time`
  channel and triggers a tick pass. Filetests advance time
  manually. The spine doesn't know what time means.

### What `TickContext` provides

- **Slot resolution.** `ctx.resolve(prop_path) -> &LpsValue`,
  walking the binding stack
  ([06](06-bindings-and-resolution.md)). Typed shortcuts:
  `ctx.resolve_u32`, `ctx.resolve_f32`, etc.
- **Slot change-tracking.** `ctx.changed_since(prop_path, frame) ->
  bool`, backed by the resolver cache. The cheap "should I re-do
  work?" check.
- **Artifact access.** `ctx.artifact() -> &dyn Artifact`,
  `ctx.artifact_changed_since(frame) -> bool`. Hot reload is
  observed here, not via an event.
- **Bus access.** Read foreign inputs, publish outputs.
- **Output writeback.** Writeable views into this node's output
  slot buffers.
- **The `NodeTree`** (read-only — the tick pass doesn't restructure).
- **The current `FrameId`** (so a node can record "I last
  reconciled at frame N" for its own caching).

### What `TickContext` does *not* provide

- `delta_ms` / `delta_secs`. If you want it, take a param.
- `Instant::now()`. The engine's notion of time goes through the bus.

**Open question — escape hatch for tightly-coupled timing.** If a
future node genuinely cannot tolerate going through the bus for
delta (e.g., per-sample audio at 48 kHz where round-tripping every
sample through a `param` lookup is too slow), we'll add a typed
`TickContext::engine_delta_secs()` accessor reading from the
canonical channel without the `param` indirection. Not adding it
now — premature optimisation, and the bus access cost should be
near-zero for hot params.

## §1.Y — No event channel (yet)

M5 ships with no event mechanism on `Node`. No `NodeEvent` enum, no
`handle_event` method.

The reasoning: with `ctx.changed_since` and
`ctx.artifact_changed_since` in the tick context, the only
remaining "events" had no concrete listeners. Artifact reload is a
pull at tick; missing artifact is engine-handled (entry → `Failed`,
no node hook). Memory pressure is its own required method
(`handle_memory_pressure`), not an event variant — different
contract (every node owes a response, no broadcast semantics).

The first real event will reintroduce the mechanism. Most likely
candidate: a top-down **`pre_destroy`** notification. Today's
children-first destroy ordering means a parent observes its
children gone before its own `destroy` runs. Some plausible future
nodes (a Stack with GPU binding handles into each effect) would
prefer to release child-bound resources *before* children are torn
down. When that arrives, we add it as a method (`pre_destroy`),
not an event, since it's targeted-and-required, not broadcast.

Other future events that might warrant a real channel: client
connection lifecycle, system sleep/wake, group-bus topology
changes. None of them have concrete consumers in M5 territory.
Build when needed.

## §1.Z — `*Props` and `PropAccess`

The impl owns a typed `*Props` struct with one field per
**produced** slot:

```rust
pub struct TextureProps {
    pub texture: Prop<TextureHandle>,           // state, recorded
    pub frame:   Prop<FrameId>,                 // state, recorded
    pub output:  Prop<TextureBuffer>,           // outputs[0], produced
}
```

`Prop<T>` is `(T, FrameId)`. The impl mutates these during `tick`
via `Prop::set` / `mark_updated`; the engine reads them via the
derived `PropAccess` impl.

**Consumed** slots (`params` and `inputs`) do *not* have `Prop<T>`
fields. Their values live in the resolver cache on `NodeEntry` and
are queried per-tick via `ctx.resolve` / `ctx.changed_since`.
([05](05-slots-and-props.md))

```rust
pub trait PropAccess {
    fn get(&self, path: &PropPath) -> Option<LpsValue>;
    fn iter_changed_since(&self, frame: FrameId)
                         -> Box<dyn Iterator<Item = (PropPath, LpsValue, FrameId)> + '_>;
}
```

The trait is derived on `*Props` via a custom derive
(`#[derive(PropAccess)]`); concrete shape lands in M4. The editor
queries via `props().get("outputs[0]")` etc.; the encoding is
invisible to the wire.

## Lifecycle invariants

1. `D::instantiate` is called exactly once per Node instance, on
   `EntryState::Pending → Alive` wake. On `Err`, the entry
   transitions to `Failed(reason)` (not `Alive`); status →
   `InitError(...)`. Resolution falls through `Failed` to
   `Slot.default`. The `NodeEntry` is retained for hot re-init on
   `config_ver` bump or memory-pressure release sequence. There is
   no separate `Node::init`; construction *is* init.
2. `Node::tick` may be called many times once `Alive`, never while
   `Pending` or `Failed`. On `Err`, status → `Error(...)`; the
   entry stays `Alive` (no demotion on tick failure). Next tick
   attempt happens once status returns to `Ok` (the impl
   reconciles internally via `ctx.changed_since` /
   `ctx.artifact_changed_since`).
3. `Node::destroy` is called once when the entry is removed.
   Cannot fail meaningfully — log on error, continue tear-down.
   Only `Alive` entries call `destroy`; `Pending` and `Failed`
   entries just decrement their `ArtifactRef` and are gone.
4. `Node::handle_memory_pressure` may be called only on `Alive`
   entries, between ticks. Idempotent; releases reconstructable
   buffers without losing essential state. Status unchanged on
   success. Distinct from the `Alive → Pending` demotion
   ([01](01-tree.md)).
5. **Config and artifact changes are not lifecycle hooks** — they're
   observed by the impl's next `tick` via `ctx.changed_since` /
   `ctx.artifact_changed_since`. The runtime's job on
   `set_property` or fs-reload is to update the parent's bindings /
   bump `content_frame` and increment `config_ver`; nothing else
   fires until tick.
6. **`Pending` entries absorb config / artifact updates passively.**
   Their stored `NodeConfig` and `ArtifactRef` are updated in-place;
   no wake is forced. The next demand-driven wake constructs the
   node against the latest config + artifact.

## Panic recovery

`tick` and `handle_memory_pressure` are wrapped in
`panic_node::catch_node_panic`. `destroy` is *not* wrapped — a
panic during destroy is a real bug with no recovery path other
than process exit. A panic surfaces as `Err(NodePanicked { ... })`,
which the tree treats as `Error(panic_msg)`. Load-bearing F-1.

## Tick order

Lazy demand-driven (existing lp-engine `ensure_texture_rendered`).
Outputs declare what they need; textures and shaders evaluate on
demand. **Subtle**: this doesn't fit "post-order tree traversal"
cleanly — a Texture might *not* be ticked if no Output asks for it.

Decision: **the spine doesn't impose an order.**
`ProjectRuntime::tick` delegates tick-order to `D::tick` via a
domain hook (legacy domain keeps lp-engine's lazy traversal; future
domains can pick post-order or different).

## Frame increment

Done by `ProjectRuntime::tick()` *before* dispatching the per-node
tick pass. `frame_id` is monotonic, never decreases. Wall-clock
time (when relevant) flows through the bus's `engine/time_secs`
channel — see §1.X. The legacy `FrameTime` mapping continues to
back this channel for the legacy domain.
