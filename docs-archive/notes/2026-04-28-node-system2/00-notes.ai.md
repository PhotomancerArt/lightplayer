# Node/artifact system v2 — pre-M1 architecture notes

Working notes for nailing down the **runtime node/artifact tree** before
starting `docs/roadmaps/2026-04-23-lp-render-mvp/m1-lpfx-runtime.md`.

The goal is: lock the basic artifact/node model so lpfx (M1) and
lp-engine (later rewire) share one runtime spine, instead of lpfx
inventing its own node graph and lp-engine keeping the old `lp-model`
flat-handle one.

Cross-references:

- Existing model: `lp-domain/lp-domain/src/{node,types,visual,...}.rs`
- Existing engine: `lp-core/lp-engine/src/{project,nodes,runtime}/`
- Existing handles/specifiers: `lp-core/lp-model/src/nodes/*`
- Domain vocabulary: `docs/design/lightplayer/{domain,notes}.md`
- Roadmap context: `docs/roadmaps/2026-04-23-lp-render-mvp/{overview,decisions,notes}.md`
- M1 starting point: `docs/roadmaps/2026-04-23-lp-render-mvp/m1-lpfx-runtime.md`

## What the user said (paraphrased and structured)

### Artifacts (the "class" side)

- An **Artifact** is the prototype of a node — `class` in the OOP
  sense, where a Node is an instance.
- `ArtifactSpec` is the on-disk reference (already exists in
  `lp-domain`: opaque string, e.g. `./fluid.pattern.toml`,
  `lib:/std/rainbow.pattern`).
- We need an **Artifact manager** that:
  - Loads / parses artifacts from `LpFs` on demand.
  - Caches the parsed form.
  - Tracks **which live nodes use each artifact** so it knows when an
    artifact is safe to unload.
  - Operates under tight memory pressure (this is embedded; we
    cannot keep every artifact resident).
- Multiple **nodes** can share one artifact (e.g. two perlin
  patterns in a timeline). The manager's refcount has to support that.

### Nodes (the "instance" side)

- A **Node** is the basic identifiable runtime object. It is an
  *instance* of an artifact in the tree.
- Nodes form a **tree**, with a single root.
- The **root** depends on the hosting context:
  - lpfx (visual subsystem on its own): root is the Visual being
    rendered — a `Pattern`, `Stack`, `Live`, or `Playlist`.
  - lp-engine (full system, future): root might be the **Show**, with
    Visuals as one child subtree and Rigs/Outputs as siblings. Or
    a `Project` above that.
  - We don't have to pick the absolute top of the tree right now;
    we just need lpfx's root to be the same shape as a deep node
    inside lp-engine's tree later.

### Per-node anatomy

The user's mental model: every node has

- **meta** — schema_version, title, kind, etc. (usually static, from
  the artifact).
- **params** — typed values, accessed by *name* (`speed`,
  `config.spacing`). Mostly data-flow leaves: editor or default
  values, or bus-bound.
- **inputs** — typed channels coming **in** to this node. Treated as
  more "first-class" than params: they are indexed (positional, not
  named?), and stacks wire upstream `output` to downstream `input`.
- **outputs** — typed channels going **out** of this node. The user's
  sketch is "there is only one primary output", which matches today's
  `Pattern`/`Effect`/`Transition` (single texture out).
  - There may be **sidecar outputs** mirroring params (e.g. a Live's
    `priority` scalar). User's analogy: "we might want some kind of
    sidecar output mirroring params."

The conceptual difference between params and inputs in this model:

- **inputs**: indexed, structural, single primary type (texture in
  the visual world); stacks wire them.
- **params**: named, semantic (typed by `Kind`), authored, can be
  bound to bus channels.

Both have similar runtime mechanics (typed value flowing in), but
they live in different namespaces because the editor and the
composition story treat them differently.

### Node children (dynamic)

- A node's children **can change** during the program's lifetime.
- Example: a `Pattern` has a `gradient` param of `Kind::Gradient`. If
  the gradient is authored as a procedural sub-`Pattern`, that
  sub-Pattern becomes a **child** of the owning node. If the user
  edits the gradient to come from the bus, the child is destroyed.
- Children are owned by the parent: parent unload ⇒ children unload.
- This is the same mechanism a `Stack` uses for its `[input]` and
  `[[effects]]`: those are children, owned by the Stack.

So child nodes can come from at least two places:

1. **Structural** children: declared by the artifact (`Stack.input`,
   `Stack.effects`, `Live.candidates`, `Playlist.entries`).
2. **Param-promoted** children: a typed param whose binding is
   `Visual` (i.e. `bind = { visual = "..." }` or analogous) becomes
   a child node providing values to that param.

### Addressing — paths and UIDs

Four addressing surfaces, each with a job:

| Surface | Who uses it | Form | Stable? |
|---|---|---|---|
| **Name** | author | string segment, e.g. `speed` | yes (per artifact) |
| **NodePath** | humans / TOML / NodeSpecs | `/main.show/fluid.pattern` | stable across runs |
| **Uid** | runtime fast-path lookups | `i32` (sequentially assigned, starts at 1) | runtime-only |
| **ArtifactSpec** | on-disk loader | opaque string, file-relative | resolves to artifact |

Already in `lp-domain`:

- `Uid(u32)` — but the user is **leaning toward `i32`** (already
  decided in lp-core) for parity with `NodeHandle(i32)` in lp-model.
- `NodePath`, `Name`, `PropPath`, `NodePropSpec`, `ArtifactSpec`,
  `ChannelName`.

**Open question:** do we change `Uid` to `i32`, keep `u32`, or
introduce a new alias? See Q1 below.

Rules the user wants nailed:

- **One-to-one** between `NodePath` and `Uid` while a node is alive.
  Path is the address; UID is the cached handle.
- Path is the **read** form (humans, TOML bindings, `NodePropSpec`).
- UID is the **lookup** form (runtime, fast).
- `ArtifactSpec` only identifies a *node* in the **loading context**
  — i.e. when a parent artifact says "instantiate this artifact as
  my child". Once the node exists, it's addressed by path / UID, not
  by the artifact spec.

### Path semantics — the part we have to define

We need clear rules for how path segments are formed for
**param-promoted** and **structural** child nodes, because today the
existing `NodePath` grammar is just `/<name>.<type>/...`, which works
for top-level nodes but doesn't cover dynamic subtrees.

Open questions (see Q3 / Q4 below):

- How does a `Stack`'s effect at index 0 appear in the tree?
  - `/main.stack/effects[0].effect` (PropPath-style index segment)?
  - `/main.stack/0.effect` (numeric segment)?
  - `/main.stack/tint.effect` (artifact-derived name)?
- Where does a `Pattern`'s gradient sub-pattern appear?
  - `/main.pattern/gradient.pattern` (param name as the segment)?
  - `/main.pattern#params.gradient` style with hash-prop?

### Lifecycle

Need a clear node lifecycle. First sketch:

```
loaded                             — artifact in the artifact manager,
                                     no nodes yet
   │
   ▼
created (in tree, has Uid+Path)    — parent has spawned child;
                                     no resources yet
   │
   ▼
initialized                        — child resources allocated
                                     (compiled shader, output texture
                                     buffer, etc.)
   │
   ▼
running                            — render() called each frame
   │
   ▼
shedding (optional)                — drop optional buffers under
                                     memory pressure (cf. lp-engine's
                                     shed_optional_buffers)
   │
   ▼
destroyed                          — removed from tree;
                                     parent unload destroys children
                                     bottom-up
```

A simpler version (skip the optional shedding rung) might be enough
for v0. But the embedded target (ESP32-C6) cares about
`shed_optional_buffers` already (see
`lp-core/lp-engine/src/nodes/mod.rs` `NodeRuntime`), so we should
plan for it.

### Constraints from the embedded target

- No allocator pressure spikes — node creation/destruction shouldn't
  fragment.
- We need to be able to **tear down a subtree** without leaks (output
  channels close, output buffers free, compiled shaders unload from
  the LPVM pool).
- Memory-stats hooks already exist (`MemoryStatsFn`); the new node
  system shouldn't lose them.

## What already exists (and how it shapes the design)

### `lp-domain::node::Node` trait

```rust
pub trait Node {
    fn uid(&self) -> Uid;
    fn path(&self) -> &NodePath;
    fn get_property(&self, prop: &PropPath) -> Result<LpsValue, DomainError>;
    fn set_property(&mut self, prop: &PropPath, value: LpsValue)
        -> Result<(), DomainError>;
}
```

This is just a property-access surface. It does not include:

- Tree structure (parent/children).
- Lifecycle (init, render, destroy).
- Artifact reference.
- Inputs / outputs (vs params).

So the M2 trait was deliberately left thin — graph topology is
intentionally not in the domain. The new system needs to layer on top.

### `lp-domain` Visuals

The artifact-side composition is already authored:

- `Pattern { shader, params }` — leaf, no children.
- `Effect { shader, input, params }` — `input` is a `VisualInput`.
- `Stack { input, effects, params }` — `input` + ordered `effects`.
- `Transition { shader, params }` — driven by host (Live/Playlist),
  inputs are conventional (`inputA`, `inputB`).
- `Live { candidates, transition, bindings }` — selection runtime.
- `Playlist { entries, transition, behavior, bindings }` —
  sequenced runtime.
- `VisualInput::{Visual(VisualInputVisual), Bus(VisualInputBus)}`
  — explicitly framed in the domain as **structural composition**,
  not a binding (per `visual_input.rs` doc comment: "[input] is
  structural composition, not a binding. ... VisualInput::Visual
  *does* instantiate a child node, which is why it lives here and
  not as a Binding variant.").

So "param can promote to a child" is **already** an architectural
decision in the domain layer — at least for the Visual-input slot.
We just have to extend it to params marked as visual-bound (e.g.
`gradient` Pattern), if we want that.

### `lp-core/lp-engine::ProjectRuntime` (the old shape)

Today's runtime in `lp-engine`:

- `BTreeMap<NodeHandle, NodeEntry>` — **flat** node map; no tree.
- Sibling-only relationships, expressed via `NodeSpecifier`
  cross-references (a `fixture` references a `texture` and an
  `output` by spec; a `shader` writes a `texture`; etc.).
- Fixed `NodeKind` enum: `Texture | Shader | Output | Fixture` (no
  `Pattern`, `Effect`, `Stack`).
- `NodeStatus`: `Created | InitError(_) | Ok | Warn(_) | Error(_)`.
- `NodeRuntime` trait: `init`, `render`, `destroy`,
  `shed_optional_buffers`, `update_config`, `handle_fs_change`,
  `as_any`/`as_any_mut`. Handles trait-object downcasting.
- `NodeEntry`: path, kind, config, config_ver, status, status_ver,
  runtime, state_ver. Versions tracked per frame for change-syncing
  to the editor.
- Lazy render via `ensure_texture_rendered` traversal: the renderer
  walks demand-driven from outputs back to shaders back to other
  shaders/textures.

This is real, working, embedded-tested code. It encodes a lot of
hard-won decisions:

- **Frame-versioned config / status / state** for editor sync.
- **shed_optional_buffers** for embedded recompile flow.
- **NodeHandle** as a `Copy` `i32`, fast lookup, sequential.
- **NodeSpecifier** (string) for authored cross-refs.
- **Lazy demand-driven render** so the graph order doesn't have to
  be precomputed.

But it also has shape mismatches with where we're going:

- Hard-coded 4-kind enum can't grow to Pattern/Effect/Stack/...
- Flat map: no parent/child ownership; subtree teardown isn't a
  thing.
- `node.json` / file-per-node-on-disk model is the "old" content
  format; the new content format is `*.pattern.toml` etc., loaded
  via `lp-domain::artifact::load`.
- `BTreeMap<NodeHandle, ...>` walk for `is_shader_writing_to(this)`
  is N²-ish; fine at v0 but not what a real tree gives you.

We should **lift the good parts** (frame versioning, shed,
Copy-handle index, lazy render) into the new node system and **drop
the bad parts** (hard-coded kinds, flat map, on-disk format
assumptions, ad-hoc cross-refs).

### lpfx today (the parallel-domain part being demolished)

- `FxModule` = one shader + one manifest + one output.
- No tree, no children, no parent. Single-instance "render this
  shader" only.
- Will be replaced by lp-domain Visual types in M1.

## Open questions

Numbered for reference. Each captures the question, context, a
suggested first answer, and what would change the answer.

### Q1: `Uid` — `u32` or `i32`?

Today `lp-domain::types::Uid(u32)`; `lp-model::NodeHandle(i32)`.
The user said "should likely become an i32, not a string, for lookup
speed — we already figured this out in lp-core."

Both work. `i32` parity with `NodeHandle` is the only strong
reason. `u32` gives one more bit of address space (irrelevant in
practice).

**Suggested:** make the runtime handle `i32`, name it `Uid` in
`lp-domain` (replacing the existing `u32` `Uid`). lp-engine's
`NodeHandle(i32)` either becomes a re-export or gets folded in.

**Resolved? — needs user.**

### Q2: Where does the tree-aware Node trait live?

Three candidates:

a. **`lp-domain`** — extend the existing `Node` trait into a tree-aware
   one. Pure: keeps the runtime contract in the domain crate.
b. **A new crate (e.g. `lp-runtime`)** — domain stays thin; runtime
   lives separately so lp-domain doesn't know about render loops or
   tree topology.
c. **`lpfx`** — keep the trait in lpfx for now; lift later when
   lp-engine adopts.

**Suggested:** (a) — extend `lp-domain`, because the user wants both
lpfx and (eventually) lp-engine to share *one* runtime spine. If
the trait shape lives in lpfx, lp-engine can't depend on it without
inverting layering.

Concrete sketch: keep the existing `Node` (property access) trait,
add a sibling `RuntimeNode` (or rename it; there's an open naming
question) with tree + lifecycle:

```rust
pub trait RuntimeNode {
    fn uid(&self) -> Uid;
    fn path(&self) -> &NodePath;
    fn artifact_kind(&self) -> &str;       // matches Artifact::KIND

    fn parent(&self) -> Option<Uid>;
    fn children(&self) -> &[Uid];

    fn meta(&self) -> &NodeMeta;           // schema_version, title, ...
    fn params(&self) -> &dyn ParamsView;   // typed get/set; named
    fn inputs(&self) -> &dyn InputsView;   // typed; indexed
    fn outputs(&self) -> &dyn OutputsView; // typed; indexed (1 main + sidecars)

    fn lifecycle(&self) -> Lifecycle;
}
```

This is a sketch only. Real shape is design phase.

**Resolved? — needs user.**

### Q3: How do params and inputs differ in the model?

User's framing: "inputs are indexed and there is only one output
(is that right?)". Let's pin this down.

**Inputs**:

- Positional / indexed (the user says).
- Currently the only "input slot" type in the domain is
  `VisualInput`, which appears on `Effect.input` (1 slot, named
  `input`) and `Stack.input` (1 slot). Stack also has a positional
  `effects: Vec<EffectRef>`, where each entry has a single
  positional input fed by the previous effect's output.
- Transition has 2 conventional inputs (`inputA`, `inputB`) but they
  aren't first-class declared — they're implicit shader uniform
  contracts.
- **Future N-arity Mixer** is in the domain doc but not yet
  implemented.

**Params**:

- Always named (`Slot` keyed by `Name`).
- Typed by `Kind` (semantic) and `Constraint` (legal range).
- Bus-bindable.

So in v0:

- Inputs are at most 2 (single-input for Effect/Stack; two for
  Transition; future N for Mixer). Indexed positionally.
- Params are arbitrary count, named.

**Suggested model** for the runtime view:

```
Node
├── meta:    NodeMeta { kind, title, schema_version, ... }
├── params:  Map<Name, ParamSlot>           (named, kind-typed, bus-bindable)
├── inputs:  Vec<InputSlot>                 (indexed, single-Kind::Texture for now)
├── outputs: Vec<OutputSlot>                (indexed, single-Kind::Texture for now)
└── children: Vec<Uid>                      (ordered)
```

Or even simpler: **params and inputs have the same underlying
slot type**, just different namespaces (a Map vs a Vec). The
indexing difference is a wrapper concern, not a slot-shape concern.

**Resolved? — needs user.**

### Q4: NodePath grammar for nested nodes

`NodePath::parse("/main.stack/fluid.pattern")` already works today.
But what about:

- A `Stack`'s effect at index 0?
  - **Option A — name-based**: each effect has its own
    artifact-derived name; segment is `tint.effect`. Issue: two
    `tint.effect`s in one Stack collide. Need a disambiguator
    (`tint_2.effect`?).
  - **Option B — position-based**: segment is `0.effect` (a numeric
    segment). Simple, stable across reorderings? No — reordering
    changes positions. Stable across the *same* TOML file though.
  - **Option C — slot-relative**: segment is `effects[0]`, treating
    the parent's `effects: Vec` as a property. Mixes
    `NodePath` and `PropPath` — possibly fine, possibly muddled.
- A `Pattern`'s `gradient` param promoted to a child Pattern?
  - Likely segment is `gradient.pattern` (param name . child kind).
- A `Live`'s candidates?
  - Same problem as Stack effects.

The cleanest precedent in the existing model is `NodePropSpec` —
`path#prop`. So the **inputs** of a Stack might be addressed as
`/main.stack#input` rather than `/main.stack/input.effect`, and the
positional effects as `/main.stack#effects[0]`. But that conflates
"a property of this node" with "a child node", which the domain
explicitly distinguishes ("[input] is structural composition, not a
binding").

**Suggested:** keep paths grammatical (slash-separated `name.type`
segments). Each child node — whether structural (effect[0]) or
param-promoted (gradient) — has a *segment name* derived from its
slot in the parent. Concrete proposal:

- Param-promoted children take the **param name**:
  `/main.pattern/gradient.pattern`.
- Structural slots that are scalar (Effect.input, Stack.input):
  `/main.stack/input.pattern`.
- Structural slots that are positional (Stack.effects):
  `/main.stack/effects_0.effect` *or* the artifact-derived name
  with a uniqueness suffix (`/main.stack/tint.effect`,
  `/main.stack/tint_2.effect`). We pick one in design.

The point is: the parent always knows how to derive the child's
segment from the slot it's filling. Path is a function of (parent,
slot). UIDs are the runtime backstop for unique identity.

**Resolved? — needs user.** This is one of the bigger design calls.

### Q5: Artifact manager — what's the surface?

Sketched API:

```rust
pub trait ArtifactManager {
    /// Resolve a spec relative to a base, parse, cache.
    fn load<A: Artifact>(
        &mut self,
        base: &Path,
        spec: &ArtifactSpec,
    ) -> Result<ArtifactHandle<A>, LoadError>;

    /// Decrement refcount. Free if zero.
    fn release<A: Artifact>(&mut self, handle: ArtifactHandle<A>);

    /// Drop everything not currently held.
    fn shed(&mut self) -> usize;
}
```

Concrete subquestions:

- Q5a: typed handle (`ArtifactHandle<Pattern>`) or untyped
  (`ArtifactHandle` + downcast)? Typed is nicer; untyped is simpler
  for the manager. Probably untyped storage with a typed wrapper
  around `Box<dyn Any>`.
- Q5b: what's the cache key? Canonical path (pre-symlink-resolved)?
  Spec string? Both (canonical path is the dedup key, spec the
  display key)?
- Q5c: who holds the manager? lpfx engine? The root node? Every
  node has access via context?
- Q5d: do we need eviction in v0? M1 says "no LRU, no eviction" —
  agreed. But we should still know **which artifact each live node
  came from**, so when M3+ adds eviction it's a small change.

**Suggested:** typed handles; canonical-path cache key; manager
held by the **engine** (one per process); no eviction in v0 but
refcount-tracking from day one.

**Resolved? — needs user.**

### Q6: Does the root Visual have the same shape as inner Visuals?

In other words: is the Pattern node at `/main.pattern` the same kind
of object as the Pattern node at `/main.stack/effects_0.effect`?

**Suggested:** yes, identical. The root has `parent() == None`;
everything else is the same. This means the engine isn't "loading a
Pattern" per se — it's "instantiating a Pattern as the root child of
the engine." The engine's job is just "host one root subtree and
render it."

This matches lp-engine's future "Show is a node, Visuals are its
children, Rigs are its other children" framing.

**Resolved? — likely yes, but worth confirming.**

### Q7: How does the bus / binding fit?

Out of scope for this design (M5 of the roadmap), but the node
system has to leave a hole that the bus can fill.

Specifically:

- A param's value at render time comes from one of:
  1. The Slot's literal default (M1).
  2. A `Binding::Bus(channel)` that reads from a `Bus` impl (M5).
  3. A child node's output (param-promoted; M4 / M6).

The Node trait should expose the param **as a typed get** (`fn
read_param(&self, name) -> Result<LpsValue>`); the binding-vs-default
resolution lives behind that.

**Suggested:** Node has `params() -> &dyn ParamsView` where
`ParamsView::read(name) -> LpsValue`. M1 reads from the slot's
default. M5 swaps the impl to "ask the bus first, fall back to
default."

**Resolved? — likely OK, just flagging.**

### Q8: Node lifecycle — explicit states or implicit via methods?

Two styles:

a. **Explicit**: `Lifecycle::{Created, Initialized, Running,
   Destroyed}` enum on the node, with state-machine transitions.
b. **Implicit**: just methods (`init`, `render`, `destroy`); the
   node tracks its own state internally.

lp-engine's `NodeRuntime` does (b) plus a separate `NodeStatus`
enum on the *NodeEntry* (not the runtime itself).

**Suggested:** keep the same split — node trait has methods, node
container (the entry / cell wrapping it) tracks status. This leaves
the trait small.

**Resolved? — needs design.**

### Q9: What's the difference between this and a game-engine scene tree?

User's reference: "this system will allow us to render a node tree,
if we want to (like in a game engine), and establishes a clear
ownership/lifecycle dominance tree."

Godot, Unity (GameObjects), Unreal (Actor + Component), etc., all do
this. Common patterns:

- **Single-rooted tree** with parent/child ownership.
- **Stable identity** — (Godot's NodePath, Unity's
  GameObject.GetInstanceID).
- **Lifecycle hooks** — `_ready`, `_process`, `_exit_tree`.
- **Property reflection** — engines expose typed get/set on nodes.
- **Composition via children** — a tank `Node` has a turret `Node`
  child that has a barrel `Node` child.
- **Process / render order** — usually depth-first, parent-first.
- **Scene = serialized tree of nodes** — analogous to our
  Pattern/Stack artifacts being authored trees.

Key differences for us:

- **Static topology mostly.** Godot scenes are dynamic at runtime;
  our trees are mostly fixed-once-loaded with editor-driven
  reshapes. Cheaper to implement.
- **Param model is heavier than Godot's.** Godot has typed
  properties; we have `Kind` + `Constraint` + `Presentation` +
  `Binding`. The slot grammar is the param model.
- **Bus is non-game-engine.** No game engine has a "channel
  routing" layer like ours; it's closer to an audio DAW or a
  modular synth (think VCV Rack, or TouchDesigner's CHOP/SOP/TOP
  network). Good prior art there.
- **Embedded constraint.** Most game engines assume a host with
  GBs of RAM. We have ~80kB on ESP32-C6 free. Artifact eviction
  matters; their resource caches don't have to.

**Implication:** the tree mechanics are well-trodden. The novelty
is *how params + bus + bindings sit on top* — and that's mostly
already designed in `lp-domain`. The runtime layer's job is to
realize the existing model into living instances.

### Q10: Naming — "Node" is overloaded already

lp-domain has `Node` (property access trait). lp-model has
`NodeHandle`, `NodeKind`, `NodeSpecifier`, `NodeConfig`. lp-engine
has `NodeRuntime`, `NodeEntry`, `NodeStatus`. The new tree-aware
trait makes a *fourth* meaning of "Node" in the codebase.

Options:

a. Embrace the overload. Rename: `Node` (lp-domain) →
   `RuntimeNode`. Add `lp-domain::node::tree::Tree` etc. This is the
   user-facing word and the design doc uses it.
b. Pick a different word for the live tree-instance. `Instance`?
   `LiveNode`? `TreeNode`? Awkward.

**Suggested:** keep "Node" as the user-facing term; rename internal
trait knobs. The `lp-domain::node::Node` trait becomes
`PropertyAccess` (or `NodeProperties`); the tree-aware trait keeps
the name `Node`. Users in docs and chat say "Node" without
qualifying.

**Resolved? — needs user.**

## What I want to confirm with the user (short list)

1. Q1: `Uid` switches to `i32`?
2. Q2: tree-aware Node trait lives in `lp-domain`?
3. Q3: are inputs and params really two different slot kinds, or
   one underlying slot type with two different namespaces?
4. Q4: path segment derivation rules for structural-positional
   children (Stack effects, Live candidates).
5. Q5: artifact manager API surface — typed handles, canonical
   path keys, refcount from day one even without eviction.
6. Q6: root vs inner nodes are the same shape (just `parent() ==
   None` for root).
7. Q10: trait naming — keep "Node" as the runtime instance word,
   rename the existing `lp-domain::node::Node` trait.

## What's already nailed (from existing code + decisions)

These don't need re-litigating:

- `NodePath` grammar: `/<name>.<type>/...`, validated in
  `types.rs`.
- `PropPath` grammar: dotted fields + bracket indices, parsed by
  `lps_shared::path`.
- `NodePropSpec`: `path#prop`, round-trip stable.
- `ArtifactSpec`: opaque file-relative string in v0.
- `ChannelName`: `<type>/<dir>[/<n>]`, convention-only validation.
- Artifact taxonomy: Pattern / Effect / Stack / Transition / Live /
  Playlist (six kinds; Mixer is future).
- `[input]` is structural (instantiates a child); `bind` is routing
  (does not).
- `ParamsTable` is implicit `Shape::Struct`.
- `Slot { shape, label, description, bind, present }` — no separate
  default field on Slot; defaults flow through Shape (M2 Q15).
- Compose-time validation, never runtime (`lightplayer/notes.md`
  Design §3).
- Bus is the seam between visuals and I/O (lp-render-mvp D2 / D8).
- The visual subsystem is `lpfx`; lp-engine wraps it later with
  Show/Rig/etc.
- Per-shader backend trait stays backend-agnostic (lp-render-mvp
  D7).
- No LRU / no eviction in v0 (M1 acceptance).

## Sketched layering (proposal, to be refined in analysis)

```
┌──────────────────────────────────────────────────────────────┐
│ lp-engine (later)                                            │
│   Show, Rig, Project — root nodes that compose lpfx          │
└──────────────────────────────────────────────────────────────┘
                       ▲
                       │ uses
                       │
┌──────────────────────────────────────────────────────────────┐
│ lpfx — visual subsystem                                      │
│   Engine: hosts one root subtree                             │
│   Node impls: PatternInstance, EffectInstance, StackInstance │
│   ArtifactCache (impl of ArtifactManager)                    │
│   ShaderBackend impls (lpvm; later wgpu)                     │
└──────────────────────────────────────────────────────────────┘
                       ▲
                       │ uses
                       │
┌──────────────────────────────────────────────────────────────┐
│ lp-domain (extended)                                         │
│   Node trait (tree-aware, lifecycle, params/inputs/outputs)  │
│   ArtifactManager trait (load, release, shed)                │
│   NodeTree container (uid → Node, parent/child links)        │
│   Existing: Slot, Kind, Constraint, Binding, Visual types,   │
│   NodePath, Uid, ArtifactSpec, ChannelName                   │
└──────────────────────────────────────────────────────────────┘
```

The **`Node` trait + `NodeTree` container + `ArtifactManager`
trait** are what need to be added to lp-domain before lpfx can
build M1. lpfx's `Engine` becomes "an `ArtifactManager` impl + a
`NodeTree` it owns + a render loop over it."

This sketch is what the analysis writeup will sharpen.
