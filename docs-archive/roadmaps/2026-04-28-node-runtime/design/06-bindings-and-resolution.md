# 06 — Bindings and resolution

> **M4.3a update:** Authored bindings live in `lpc-source` as
> source-side binding/value-spec types. Wire-safe literal payloads use
> `lpc_model::ModelValue`; runtime resolution in `lpc-engine` produces
> `LpsValueF32` and converts only at the wire boundary.

A binding is a *connection*, on a slot, that says "instead of using
the slot's default value, take it from this source." Resolution is
the per-tick walk that turns each consumed slot into a current
value.

## `Binding` enum

```rust
#[derive(Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Binding {
    /// Read or write the named bus channel.
    Bus(ChannelName),                  // bind = { bus = "audio/in/0/level" }

    /// Inline literal / texture recipe. Authored portable form carries
    /// `ModelValue` (inside `SrcValueSpec` / legacy `ValueSpec`); resolves to
    /// `LpsValueF32` only inside `lpc-engine`.
    Literal(ValueSpec),                // bind = { literal = 0.7 } — rename → SrcValueSpec

    /// Read another node's output slot.
    NodeProp(NodePropSpec),            // bind = { node = { node = "...", prop = "outputs[0]" } }
}

/// Note: the implementation reuses the existing `NodePropSpec` type (defined in
/// `lpc-model::node::node_prop_spec`) rather than introducing a new `NodePropRef`.
/// Both have the same shape: `{ node: TreePath, prop: PropPath }`.
#[derive(Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NodePropSpec {
    pub node: TreePath,                // absolute path; relative addressing
                                       // is a future binding-resolver feature.
    pub prop: PropPath,                // "outputs[0]" or "outputs[0].rgb" etc.
}
```

> **Implementation note:** `Binding::Literal` stores the authored portable recipe
> — today **`SrcValueSpec`** (historically `ValueSpec`); literal payloads use **`ModelValue`** shapes — not bare **`LpsValueF32`** (handles are runtime-only).
> Older text contrasted against `Binding::Literal(LpsValue)`; same boundary,
> sharper type names ([`../m4.3a-crate-split-wire-value/plan.md`](../m4.3a-crate-split-wire-value/plan.md)).

Three variants. Three is enough.

- **`Bus`** — the modular-synth path. Implicit channels exist
  when at least one binding references them; direction (read vs
  write) is contextual to the slot's role
  (`docs/design/lightplayer/quantity.md` §8). M5 ships a stub
  bus (`HashMap<ChannelName, Option<NodeId>>`); real multi-bus
  is deferred ([prior-art](../m1-prior-art/synthesis.md)).
- **`Literal`** — the constant. Authoring shorthand: a bare value
  in TOML (e.g. `scale = 6.0`) parses to `Binding::Literal`
  ([04](04-config.md) §Authoring shorthand).
- **`NodeProp`** — the direct edge. Used heavily by the `Inline`
  desugaring ([01](01-tree.md)) and by authors who want explicit
  patching.

What **isn't** a binding variant:

- **`Visual`** (the inline-child sugar `bind = { visual = "..." }`)
  is a loader-only construct that desugars to `(spawn child,
  Binding::NodeProp)`. Runtime never sees it.
- **`Multi-bus`** — when needed, either grow `Bus(ChannelName)` to
  take a `BusKind { kind, name }` struct or add a `BusOn` variant.
  Either is purely additive.

## Where bindings live

Three layers, listed in resolution priority:

1. **Per-instance overrides.** `NodeConfig.overrides:
   BTreeMap<PropPath, Binding>` ([04](04-config.md)). Set by the
   parent's TOML use-site or by the editor.
2. **Artifact `bind`.** `Slot.bind: Option<Binding>`. The artifact
   author's hint about the natural source. `Kind::default_bind`
   populates it for kinds like `Instant` (→ `Bus("time")`),
   `AudioLevel` (→ `Bus("audio/in/0/level")`).
3. **Slot default.** `Slot.default` materialised via
   `Slot::default_value`. The literal floor; always present.

## Pull-based resolution

```rust
impl<'a> TickContext<'a> {
    /// Get the current value of `prop` on this node.
    /// Cached on the node's resolver_cache; recomputed on cache miss.
    pub fn resolve(&mut self, prop: &PropPath) -> &LpsValue { … }

    /// Did this slot's value change since `since`?
    pub fn changed_since(&self, prop: &PropPath, since: FrameId) -> bool { … }

    /// Did the artifact (TOML reload) change since `since`?
    pub fn artifact_changed_since(&self, since: FrameId) -> bool { … }
}
```

Resolution algorithm for `prop`:

```text
fn resolve(entry, prop, frame):
    if cached and cache_age(prop) >= max(content_frame, config_ver,
                                         dep_frame_for(prop)):
        return cached.value

    1. Check entry.config.overrides[prop]:
        - Bus(ch)        → read bus[ch]; if Some,  done.
        - Literal(v)     → done.
        - NodeProp(ref)  → resolve target node's outputs/state slot:
                              if Failed/Pending → fall through
                              else              → done.
    2. Check artifact.slot_at(prop).bind:
        - same dispatch as overrides; same fall-through.
    3. Materialise artifact.slot_at(prop).default → done.

    cache result with current frame; return value.
```

**Per-slot priority is replace, not stack.** When an override is
set, it *replaces* the artifact `bind` entirely. Silently falling
through to the artifact's bind when an override binding's source
is empty (e.g., audio drops out) would surprise the author. The
literal `default` is the universal floor.

**Within a layer, on bind failure, fall through.** If
`overrides[prop] = Bus(missing_channel)`, resolution does NOT fall
back to the artifact's `bind` — it falls to `default`. (The
override "you said *this* source"; if that source isn't producing,
default is the agreed neutral.)

This three-level cascade-fallthrough is the whole resolution rule.
One sentence:

> The active binding is whichever is highest priority (override >
> artifact bind) and produces a value; if neither does, return the
> slot's default.

## Cache invalidation

Per-slot cached value is stale if any of these advance past
`cached.changed_frame`:

- **`entry.config_ver`** — override layer changed.
- **`artifact.content_frame`** — artifact reloaded; the slot's
  default or `bind` may have changed.
- **For `NodeProp` bindings:** the target node's
  `props().get(target_prop_path).changed_frame` advanced.
- **For `Bus` bindings:** the writer's `changed_frame` for that
  channel advanced.

Implementation: store `cached.changed_frame` per entry, invalidate
the cache entry on changes; recompute on next access. The
specific update mechanism (proactive sweep vs lazy invalidation)
is implementation detail — both produce correct
`changed_since(prop, frame)` answers.

## Bus

The bus is *not* a node. It's a runtime registry of channels:

```rust
pub struct Bus {
    channels: HashMap<ChannelName, ChannelEntry>,
}

pub struct ChannelEntry {
    /// The current writer (a NodeId whose outputs[N] feeds this channel),
    /// or None if no writer yet.
    pub writer: Option<(NodeId, PropPath)>,
    /// Last value cached for cross-tick stability + readers without
    /// a publishing writer this frame.
    pub last_value: Option<LpsValue>,
    pub last_writer_frame: FrameId,
    /// Channel kind, established by first reader/writer.
    pub kind: Option<Kind>,
}
```

M5 ships a stub bus: just enough machinery for the
`time` / `engine/time_secs` channel and the legacy bridge's
target_textures-as-bus translation. Multi-bus
(Local / Group / Sync / Flock) is a separate roadmap.

### Bus direction is contextual

A `Binding::Bus(ch)` on a `params` slot is a **read** (consumed →
fetch from channel). On an `outputs` slot it would be a **write**
(produced → publish to channel). The slot's namespace decides
direction; the `Binding` enum doesn't carry it. Same as
`docs/design/lightplayer/quantity.md` §8.

### Bus channel typing

The first reader/writer on a channel **declares** its `Kind`.
Subsequent users must match; mismatches → `NodeStatus::Warn`,
default fallthrough. Compose-time strict validation lands with the
binding-resolver redesign; M5 is lenient.

## `NodeProp` resolution

For `Binding::NodeProp(NodePropRef { node, prop })`:

1. **Look up `node` in the tree.** Absent → resolution warns,
   default fallthrough. (The path may correspond to an artifact
   that isn't loaded yet; the warning surfaces; resolution
   periodically retries on `config_ver` bumps.)
2. **Find the entry's `EntryState`:**
   - `Pending` — wake it (`D::instantiate`). On success → Alive,
     proceed. On failure → Failed; default fallthrough.
   - `Alive` — proceed.
   - `Failed` — default fallthrough; resolution records the cause
     in the binding's source so editor can surface "this binding
     points to a failed node."
3. **Look up `prop` in `entry.props().get(prop)`.** None →
   default fallthrough. Some(v) → use v.

**Targets are outputs and state, not params/inputs.** The slot's
namespace must be `outputs` or `state` (see [05](05-slots-and-props.md)).
Targeting `params` or `inputs` is rejected during config-load —
they're sink-side (consumed). State is introspectable but not
bindable; M5 also rejects state targets.

> M4.3's config-load enforces this in one line using
> `NodePropSpec::target_namespace()` (shipped in M4.2). The helper
> projects any `PropPath` to `Option<PropNamespace>` —
> `Params | Inputs | Outputs | State` — so the loader can assert
> `target_namespace() == Some(Outputs)` (or later, allow `State`).

(O-2 resolved: outputs only for now. May relax to state later if a
real use case arrives.)

## Cycles

A cycle through `NodeProp` bindings (`A.outputs[0]` ← `B.params.x`,
`B.outputs[0]` ← `A.params.x`) is detected during binding-graph
build-up:

- M5: forbidden. Compile-time error during config-load; the
  config that would close the cycle returns `Err`. Status of the
  affected nodes → `InitError("cyclic binding")`.
- Future: read **last frame's value** for backward edges in a
  detected cycle. That handles feedback (an input bound to its
  own output for one-frame delay) elegantly, at the cost of an
  implicit per-cycle-edge buffer of last-frame values. Defer the
  implementation; the binding-graph builder and resolver both
  carry the design intent forward.

## Cascade `[bindings]` (Live)

```toml
[bindings]
"candidates/0#emitter_x" = { bus = "touch/in/0/x" }
```

Cascade entries on the `Live` artifact specify bindings on
descendant nodes. **Materialised at parent-init**: each cascade
entry is parsed (`<NodePath>#<PropPath>`), resolved against the
descendant tree, and inserted into the descendant's
`NodeConfig.overrides`. The resolver never sees a cascade — it
sees a normal override on the descendant.

**Conflict precedence (when both ancestor cascade and descendant
direct authoring target the same slot):** descendant-direct wins
([04](04-config.md) §Cascade).

**Reload path:** ancestor's `config_ver` bump triggers
re-materialisation of the cascade, which propagates `config_ver`
bumps to affected descendants. Pull-at-tick on each descendant
picks it up.

## What's **not** in M5 resolution

Deferred to lp-vis or beyond:

- **Multi-bus topology.** Local / Group / Sync / Flock channels.
  M5: only Local (single-host).
- **Cross-host channels.** No network bus.
- **Binding language.** `[params.speed.bind] = "audio/in/0/level
  * 0.5"` (DSL on the bind). M5: only the three enum variants.
- **Time-varying bindings.** A binding that switches source over
  time. M5: bindings are static between `config_ver` bumps.
- **Conditional bindings.** `bind = { if = "ratio < 0.5", then =
  ..., else = ... }`. Not in M5.

## Implementation notes

- **`resolve` returns `&LpsValue`** (not `LpsValue`) — values live
  in the cache. The cache is `&mut` accessible because resolution
  may need to populate. This means tick code that does
  `ctx.resolve(a); ctx.resolve(b)` needs `b` to not borrow-conflict
  with the value held from `a` — which is fine because each call
  returns a fresh shared borrow into the same map.
- **Typed shortcuts** like `ctx.resolve_f32(prop)` unwrap the
  `LpsValue::F32` variant and return `f32`, panicking on shape
  mismatch (which would be a config-load error). The wire never
  sees a panic; this is internal-consistency only.
- **Cache size.** Bounded by the number of slots a node has. For
  legacy nodes, 0–10 per node. For visual nodes, 5–50. Total cache
  per project is `O(num_nodes * avg_slots)`. ESP32-acceptable.
- **No cross-frame value memoisation by default.** If a binding's
  source hasn't changed since last frame, resolution returns the
  cached value with the cached frame — no recomputation. That's
  the hot path.

## Open questions

- **Resolver-cache sharing across read paths.** The editor reads
  `props().get` (which goes through `PropAccess`) for produced
  values, but for consumed values it'd want the resolver cache.
  Not yet decided whether `props().get` falls back to the
  resolver cache for consumed slots, or whether the wire only
  exposes produced. Lean: produced-only, since that's what the
  editor needs to display "current value of node X" — params are
  authored data the editor already has on the client side.
  Pin in M4.
- **Bus writer vs publisher.** Some channels have natural single
  writers (`engine/time_secs`); others (`audio/in/0/level`) have
  external writers (audio device → bus). The bus needs an
  insertion API for both: `Bus::publish(channel, value, frame)`
  for engine-internal, plus an external-writer registration for
  hardware sources. Pin in M5 stub-bus implementation.
- **Stale `last_value`.** A reader on a channel that hasn't been
  written for many frames — does it see `Some(stale_value)` or
  `None`? Lean: `Some` (channels are persistent). With a TTL for
  channels that should be transient (audio levels, gestures); M5
  defers TTL.
