# Spine design — node-runtime roadmap

**Milestone:** M3 — Spine design pass
**Reads:** [`notes.md`](notes.md) (strawman + decisions log),
[`prior-art.md`](prior-art.md) (M1 synthesis), the as-built M2 code
(`lp-core/lpc-runtime/`, `lp-core/lpc-model/`, `lp-legacy/lpl-*/`).
**Drives:** M4 (artifact spine — class side), M5 (node spine + sync
cutover — instance side), M6 (cleanup + validation).

This document is the binding shape decision for the spine. M4 and M5
implement against this; if implementation reveals a mistake here, this
doc gets edited (with a "design erratum" entry below) rather than M4 /
M5 silently diverging.

This is **not** an implementation plan. There are no phases, no commit
plans, no checklists. Where a section sketches a Rust signature, it's
to pin down a decision, not to dictate the final code.

## §0 — Bird's-eye summary

The spine is two layers, three crates, four namespaces:

```
                 ┌─────────────────────────────────────────────────┐
                 │  lpc-runtime — generic spine                    │
                 │                                                 │
                 │  ProjectRuntime<D: ProjectDomain>               │
                 │      ├─ NodeTree (Uid <-> NodePath, parent      │
                 │      │            <-> children, sibling-unique) │
                 │      ├─ ArtifactManager<D::Artifact>            │
                 │      ├─ frame versioning + change events        │
                 │      ├─ panic recovery + shed                   │
                 │      └─ fs-watch routing                        │
                 │                                                 │
                 │  Node trait — tree + lifecycle + slot views     │
                 └────────────────────┬────────────────────────────┘
                                      │ implemented by
                ┌─────────────────────┴────────────────────────────┐
                ▼                                                  ▼
   ┌──────────────────────────────┐          ┌──────────────────────────────┐
   │  lpl-runtime — legacy domain │          │  lpv-runtime — visual domain │
   │                              │          │  (next roadmap)              │
   │  LegacyDomain: ProjectDomain │          │  VisualDomain: ProjectDomain │
   │  TextureRuntime impl Node    │          │  PatternRuntime impl Node    │
   │  ShaderRuntime  impl Node    │          │  EffectRuntime  impl Node    │
   │  OutputRuntime  impl Node    │          │  StackRuntime   impl Node    │
   │  FixtureRuntime impl Node    │          │  ...                         │
   └──────────────────────────────┘          └──────────────────────────────┘
```

The four namespaces (per node):

```
Node {
   params:  named   slots, kind-typed, bus-bindable, can promote to children
   inputs:  indexed slots, structural composition, child Uids
   outputs: indexed slots, primary product (texture, channel buffer, ...)
   state:   named   slots, sidecar runtime state, edit-only via debug hooks
}
```

### What's load-bearing-novel

Per [`prior-art.md`](prior-art.md) F-1 / F-2:

1. Client / server architecture with frame-versioned wire sync.
2. Per-node panic-recovery isolation.
3. Unified `NodeStatus` enum on the container (not on `Node`).
4. Param-promoted-to-child (no prior art; designed in §7).

The other 7 surfaces have strong prior art (Godot lifecycle / paths,
Bevy `Handle<T>` for refcount, LX vocabulary + `Placeholder` for missing
artifacts, LX `addLegacyParameter` for migrations). We adopt those.

### What this design pass changes from the M2-as-built code

Three M2 deviations from the move-map are resolved here:

- **F-M2-1 — `lpc-runtime` depends on `lpl-model`.** Resolved by
  parameterising `ProjectRuntime` over `D: ProjectDomain`. M4 strips
  `lpl-model` from `lpc-runtime`'s deps. See §3 + §11.
- **F-M2-2 — `ProjectHooks` global state.** Resolved by the same `D:
  ProjectDomain` generic. The trait stays; the singleton goes. See §11.
- **F-M2-3 — Hardcoded `texture / shader / output / fixture` suffix
  list in `lpc-runtime/src/project/loader.rs`.** Resolved by moving
  the loader's per-domain bits into `D::node_kind_from_path` and
  shrinking `lpc-runtime`'s loader to a domain-agnostic
  filesystem walker. See §11.

## §1 — `Node` trait

The spine has two traits with distinct jobs:

- **`lpc_model::NodeProperties`** (already exists, M2). Object-safe
  property reflection (`uid`, `path`, `get_property`, `set_property`).
  Editor-facing.
- **`lpc_runtime::Node`** (new, M5). Object-safe runtime spine: tree
  awareness + lifecycle + slot views + sidecar state. Engine-facing.

Concrete node types implement both. The traits are kept separate so a
property-only consumer (e.g., a TOML editor with no live runtime) can
depend only on `lpc-model`.

### `Node` (in `lpc-runtime`)

```rust
pub trait Node: NodeProperties + Send + Sync {
    /// Parent in the tree, or None for the root.
    fn parent(&self) -> Option<Uid>;
    /// Ordered children (structural + param-promoted, see §7).
    fn children(&self) -> &[Uid];

    /// The four namespaces. All return read-only views; mutation
    /// goes through set_property + slot rebinds.
    fn params(&self)  -> &dyn SlotView<Slot>;       // named
    fn inputs(&self)  -> &dyn SlotView<Slot>;       // indexed
    fn outputs(&self) -> &dyn SlotView<Slot>;       // indexed
    fn state(&self)   -> &dyn SidecarState;         // opaque blob

    /// Lifecycle (carried from existing `NodeRuntime`).
    fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error>;
    fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error>;
    fn destroy(&mut self, ctx: &dyn DestroyContext) -> Result<(), Error>;
    fn shed_optional_buffers(&mut self, ctx: &dyn ShedContext) -> Result<(), Error>;
    fn update_config(&mut self, cfg: &dyn NodeConfig, ctx: &dyn NodeInitContext)
        -> Result<(), Error>;
    fn handle_fs_change(&mut self, change: &FsChange, ctx: &dyn NodeInitContext)
        -> Result<(), Error>;

    /// Erased downcast for the legacy bridge (M5) and editor probes.
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
```

### Decisions

- **Two traits, not one.** `NodeProperties` stays in `lpc-model` (no
  runtime deps); `Node` extends it in `lpc-runtime` with everything
  that requires the engine. A `dyn Node` is a `dyn NodeProperties`
  via the supertrait bound, so editor code that only wants properties
  doesn't pay for the spine's deps.
- **Lifecycle method names match the existing `NodeRuntime` trait.**
  We're not bikeshedding `_ready` / `_enter_tree` etc. (Godot) just
  to look more like Godot. The shape *is* Godot's bottom-up
  ordering — that's enforced by the tree, not the names.
- **Bottom-up `init` ordering** per Godot's `_propagate_ready`
  (prior-art §1). Implementation: `NodeTree::init_subtree(uid)`
  recurses children first, then calls the parent's `init`. A parent's
  `init` may assume all descendants are initialised.
- **Top-down `destroy` ordering** (reverse). Children destroy first;
  parents observe a clean teardown.
- **Per-frame hook is `render`, opt-in via slot output.** Per
  prior-art §1 ("avoid universal per-tick callbacks"). Nodes with no
  outputs are not visited by the render pass. The decision of "should
  this node tick" is structural, not a flag.
- **`shed_optional_buffers` stays.** lp-engine's existing semantic
  (drop everything that can be rebuilt; rebuild on next `render` /
  `init`) survives. Used before shader recompile on ESP32.
- **Panic isolation around `init`, `render`, `update_config`,
  `handle_fs_change`.** `destroy` and `shed_optional_buffers` are
  *not* wrapped — a panic during destroy is a real bug (no recovery
  path other than process exit). All hooks are called via
  `panic_node::catch_node_panic` (existing crate) when the
  `panic-recovery` feature is on.
- **No async at this layer.** Embedded targets are single-thread; the
  client/server async story lives in `lp-server` / `lp-client`.

### `SlotView`

```rust
pub trait SlotView<S: ?Sized> {
    fn len(&self) -> usize;
    fn get(&self, idx: usize) -> Option<&S>;
    fn iter(&self) -> SlotIter<'_, S>;

    /// Named lookup; returns None for indexed namespaces (inputs/outputs).
    fn get_named(&self, name: &Name) -> Option<&S>;
}
```

Same trait for all four namespaces; each returns its own slice. Named
namespaces (`params`, `state`) implement `get_named`; indexed
namespaces (`inputs`, `outputs`) return `None` from `get_named`. M4
may add a marker-trait split (`NamedNamespace` / `IndexedNamespace`)
if usage proves it pays.

## §2 — `NodeTree` container

```rust
pub struct NodeTree {
    nodes:    Vec<Option<NodeEntry>>,                // indexed by Uid.0
    next_uid: u32,
    by_path:  HashMap<NodePath, Uid>,                // O(1) path lookup
    by_sibling: HashMap<(Uid /* parent */, Name), Uid>, // sibling-uniqueness index
    root:     Uid,
}

pub struct NodeEntry {
    pub uid:        Uid,
    pub path:       NodePath,                       // canonical absolute
    pub parent:     Option<Uid>,
    pub children:   Vec<Uid>,                        // ordered
    pub child_kinds: Vec<ChildKind>,                 // see §7

    pub status:     NodeStatus,                       // Created | InitError | Ok | Warn | Error
    pub status_ver: FrameId,
    pub config_ver: FrameId,
    pub state_ver:  FrameId,

    pub node:       Option<Box<dyn Node>>,            // None until initialised
    pub config:     Box<dyn NodeConfig>,              // authored config (for hot-reload)
    pub spec:       ArtifactSpec,                     // for diff + reload
}
```

### Decisions

- **Flat `Vec<Option<NodeEntry>>` indexed by `Uid.0`** (per F-2, plus
  prior-art §2 "O(1) HashMap for child-by-name lookup"). Tombstones
  (`None` slots) on destroy; `next_uid` monotonic, no reuse. We don't
  adopt generational indices — embedded scale doesn't justify the
  4-byte / handle cost (prior-art §2 "Generational id is optional").
- **`HashMap<NodePath, Uid>`** for path → uid resolution. Built and
  maintained as nodes are added / moved / removed. Editor and sync
  layer use this; render path uses `Uid` directly.
- **Sibling-name uniqueness enforced at add-child time** (prior-art
  §2 F-6). `add_child(parent, name, ...)` returns
  `Err(SiblingNameCollision)` if `(parent, name)` is already in
  `by_sibling`.
- **Persistence: paths, not uids** (prior-art §2 "What to copy"). TOML
  references children by `NodePath`; runtime resolves on load.
- **Status enum on the container, not on `Node`** (load-bearing F-1).
  `Node::init`'s `Result<(), Error>` is observed by the tree and
  recorded in the entry's `status`. Nodes never set their own status.
- **Frame versioning on the container** (load-bearing F-1).
  `config_ver`, `status_ver`, `state_ver` all live on `NodeEntry`,
  not on `Node`. The tree increments them when the corresponding
  thing is touched. Sync layer reads these for diffing.
- **Tombstones over compaction.** Destroy sets the slot to `None`;
  the slot stays. Reconstruction (e.g., a Pattern reappears on disk)
  *gets a new `Uid`* and re-resolves the path. No id reuse, no
  generational handling — the path is the persistent identity.
- **Tree iteration:**
  - Render: depth-first, children-first (post-order). Inputs feed
    parents; parents composite. Per prior-art §8 "Push from child to
    parent for render data."
  - Destroy: depth-first, children-first too (so a parent's destroy
    sees a fully-destroyed subtree).
  - Init: depth-first, children-first (Godot bottom-up `_ready`).
  - Editor / sync diff: caller-driven, no tree-imposed order.

### What stays on `Node`, what goes on `NodeEntry`

| Lives on `Node` (the impl)        | Lives on `NodeEntry` (the container) |
|-----------------------------------|--------------------------------------|
| identity (`uid`, `path`, `parent`) — accessor; tree owns the data | identity (`uid`, `path`, `parent`) — actual storage |
| `children() -> &[Uid]` accessor   | `children: Vec<Uid>` storage         |
| slot views                         | nothing (slots are owned by the impl) |
| sidecar `state`                   | nothing                               |
| lifecycle methods                 | `status`, `*_ver`                     |
| panic-recovery wrap location      | panic-recovery dispatch + status update |

The accessors on `Node` are convenience reads (the impl already needs
to know its own `uid` for logging etc.). Source of truth is the tree.

## §3 — `ArtifactManager`

Generic over the domain's artifact type:

```rust
pub trait ProjectDomain: Send + Sync + 'static {
    /// The domain's artifact union.
    type Artifact: Artifact;

    /// The domain's response payload (legacy: SerializableProjectResponse).
    type Response: Serialize + DeserializeOwned + Clone + 'static;

    /// The domain's node-config union (legacy: lpl_model::NodeConfig dyn).
    type Config: NodeConfig;

    /// Recognise a node directory by path suffix. Returns the artifact
    /// kind discriminant the domain uses internally. Used by the
    /// (domain-agnostic) filesystem walker in `lpc-runtime::loader`.
    fn node_kind_from_path(path: &LpPath) -> Option<<Self::Artifact as Artifact>::Kind>;

    /// Construct a fresh node from a parsed artifact + config.
    fn instantiate(&self, artifact: &Self::Artifact, cfg: &Self::Config)
        -> Result<Box<dyn Node>, Error>;

    /// Build the response payload for `get_changes`.
    fn build_response(
        &self, tree: &NodeTree, since: FrameId, spec: &ApiNodeSpecifier,
        theoretical_fps: Option<f32>,
    ) -> Result<Self::Response, Error>;
}
```

```rust
pub struct ArtifactManager<D: ProjectDomain> {
    cache:    HashMap<ArtifactSpec, Entry<D::Artifact>>,
    fs:       Rc<RefCell<dyn LpFs>>,
}

struct Entry<A: Artifact> {
    artifact:  Arc<A>,                   // Arc not because of threads —
                                         // because StrongHandle clones cheap.
    refcount:  AtomicU32,                // approximated; see "drop" below.
    placeholder: Option<TomlBlob>,       // if load failed (LX-style preserve)
}

pub struct ArtifactRef<A: Artifact> {
    inner: Arc<A>,
    on_drop: ArtifactDrop<A>,             // captures spec + manager handle
}
```

### Decisions

- **`Handle<T>` shape from Bevy** (prior-art §3): two flavours.
  - `ArtifactSpec` (string) is the persisted weak handle.
  - `ArtifactRef<A>` is the strong handle held by the live node.
- **Synchronous Drop** (prior-art §3 — Godot `Ref<T>` over Bevy's
  channel). On `ArtifactRef::drop`: refcount-decrement; if zero and
  no `Placeholder`, evict from the cache immediately.
  - This means `ArtifactManager` can't be held over an
    `ArtifactRef::drop` call as a `&mut`. We hold it as
    `Rc<RefCell<ArtifactManager<D>>>`; `Drop::drop` borrows it
    mutably and evicts.
- **Hot reload: replace content, keep handle** (prior-art §3, F-3).
  fs-watch detects modify; `ArtifactManager::reload(spec)` replaces
  the `Arc<A>` content (or, if interior-mutability cost is too high,
  swaps the whole `Arc<A>` and bumps a `version` on the `Entry`;
  consumers re-read on `version` change). Existing nodes hold
  `Arc<A>`s — they keep working until they observe the change event.
- **`Placeholder` for missing artifact** (prior-art §9, LX). When
  `ArtifactManager::load(spec)` fails:
  1. Entry created with `placeholder: Some(toml_blob)`,
     `artifact: None`-equivalent.
  2. `Node::init` returns `Err(InitError::MissingArtifact { spec })`.
  3. NodeEntry status becomes `InitError(spec)`.
  4. Re-saving a project round-trips the placeholder TOML untouched.
  5. fs-watch detecting the artifact reappearing triggers
     `ArtifactManager::reload(spec)`, which promotes the placeholder
     to a real load and notifies the affected NodeEntry to re-init.
- **Refcount eviction is *immediate*** (Godot `Ref<T>`). No deferred
  GC, no end-of-frame sweep. Node destroy → `ArtifactRef::drop` →
  refcount-zero → evict. Predictable embedded behaviour.
- **No "shedding" of artifacts under memory pressure in this
  roadmap.** Shedding *node buffers* via `shed_optional_buffers`
  stays. Shedding artifacts (releasing parsed TOML to free heap when
  refcount-zero is enforced anyway) is naturally subsumed: refcount-
  zero already evicts. If memory pressure means we want to evict
  *zero-refcount-but-cached* artifacts proactively, that's a future
  addition; not blocking M5.

## §4 — Slot views

Four namespaces — `params`, `inputs`, `outputs`, `state` — confirmed
by the strawman, prior-art §6 / §7, and M2's slot model
(`lpc_model::Slot`).

### Decisions

- **One `Slot` type, four namespaces.** No marker types. Each
  namespace differs only in **storage shape** + **default
  presentation**:

  | Namespace | Storage   | Naming  | Bus-bindable | Default presentation |
  |-----------|-----------|---------|--------------|----------------------|
  | `params`  | `Vec<(Name, Slot)>` | named   | yes          | Author UI           |
  | `inputs`  | `Vec<Slot>`         | indexed | no           | Wire tip            |
  | `outputs` | `Vec<Slot>`         | indexed | no           | Wire root           |
  | `state`   | `Vec<(Name, Slot)>` | named   | no           | Debug-only          |

  The `Slot` itself doesn't know which namespace it lives in. Validation
  (e.g., "this `params` slot can be bus-bound") is enforced by the
  namespace accessor's add / set methods, not by the slot type.
- **Param-to-child promotion is a property of the slot's
  `Binding`, not of the namespace.** §7 covers the mechanics. The
  namespace doesn't change.
- **Editor sets via `NodeProperties::set_property(prop_path, value)`.**
  `prop_path = "params.gradient"` resolves to the params namespace
  + key "gradient". `set_property` validates against the slot's
  `Constraint` and `Kind` and updates atomically. On success, the
  tree increments `config_ver` for the entry.
- **No `#[derive(Reflect)]`.** Slot grammar is *already* a richer
  schema than Bevy's reflect or Godot's PropertyInfo (prior-art §6
  "Slot grammar as the reflection schema"). Don't add a parallel
  mechanism.

## §5 — Lifecycle / status / frame versioning

These three concerns are interlocked in lp-engine today; M3 keeps
them interlocked but draws a sharper line about what's tree-side
versus node-side.

### Decisions

- **`NodeStatus` enum** (existing): `Created | InitError(String) |
  Ok | Warn(String) | Error(String)`. Stays as-is. Lives on
  `NodeEntry`. `String` payloads are TOML-friendly + cheap. (F-1.)
- **`status_ver`, `config_ver`, `state_ver`: `FrameId` on each
  entry.** Updated by the tree on every transition; the change
  event fires when a `*_ver` advances. Sync layer reads `since
  > FrameId` to find dirty entries. (F-1.)
- **Lifecycle invariants:**
  1. `Node::init` is called exactly once per Node instance, from
     `NodeTree::init_subtree`. On `Err`, status → `InitError(...)`,
     Node is dropped, NodeEntry retained as a placeholder for hot
     re-init.
  2. `Node::render` may be called many times after a successful
     `init`, never before. On `Err`, status → `Error(...)`. Next
     render attempt only happens after status returns to `Ok`
     (config update, fs change resolution, etc.).
  3. `Node::destroy` is called once when the entry is removed.
     Cannot fail meaningfully — log on error, continue tear-down.
  4. `Node::update_config` may transition `Ok ↔ Warn ↔ Error` or
     trigger re-init. Implementation chooses; tree records the new
     status.
  5. `Node::handle_fs_change` is for non-config files (e.g.,
     `*.glsl` for shaders). Same status transitions as
     `update_config`.
  6. `Node::shed_optional_buffers` is idempotent and side-effect-
     free except for memory release. Status unchanged.
- **Panic recovery:** All five hooks except `destroy` and
  `shed_optional_buffers` are wrapped in
  `panic_node::catch_node_panic`. A panic surfaces as
  `Err(NodePanicked { ... })`, which the tree treats as
  `Error(panic_msg)`. (F-1.)
- **Frame increment:** Done by `ProjectRuntime::tick(delta_ms)`
  *before* dispatching renders. `frame_id` is monotonic, never
  decreases. Per-FrameId mapping to wall-clock time lives in
  `FrameTime` (existing).
- **Render order:** Lazy demand-driven (existing lp-engine
  `ensure_texture_rendered`). Outputs declare what they need;
  textures and shaders evaluate on demand. **Subtle**: this
  doesn't fit "post-order tree traversal" cleanly — a Texture
  might *not* be rendered if no Output asks for it. Decision:
  the spine doesn't impose an order; `ProjectRuntime::tick`
  delegates render-order to `D::tick` via a domain hook (legacy
  domain keeps lp-engine's lazy traversal; future domains can
  pick post-order or different).

## §6 — `NodePath` and `PropPath` grammars

### `NodePath`

Grammar (final):

```
node_path  := absolute | relative
absolute   := "/" segments?
relative   := segments
segments   := segment ("/" segments)?
segment    := unique | parent | name
unique     := "%" name_part                 ; %ScopedName, Godot's "unique name"
parent     := ".."
name       := name_part "." kind            ; e.g. "main.stack"
                                            ; or "effects_0.effect"
                                            ; or "gradient.pattern"
name_part  := [A-Za-z_][A-Za-z0-9_]*       ; underscore-friendly
kind       := [a-z][a-z0-9_]*               ; artifact kind, lowercase
```

### Decisions

- **Godot's shape, with three deltas** (prior-art §2):
  1. Strict sibling-name uniqueness *enforced* (Godot doesn't).
  2. Underscore-form indexed segments (`effects_0.effect`), not
     bracketed (`effects[0]`). Avoids `Name` grammar churn (Q-C).
  3. No 1-indexing (LX-style); 0-indexed throughout.
- **`%name` "unique name" lookup** (Godot's idea): a node can be
  *named-unique* in its scope (the project root by default, with
  scope override at the `Show`-level later). Editor uses `%`
  paths in TOML to express "this binding follows the renamed
  node, not the original location." Implementation deferred to
  M4; reserved syntax now.
- **Segment naming, by source:**
  | Source                         | Segment form          | Example                 |
  |--------------------------------|-----------------------|-------------------------|
  | Top-level                      | `<name>.<kind>`       | `/main.stack`           |
  | Param-promoted (named slot)    | `<paramname>.<kind>`  | `/main.stack/main.pattern/gradient.pattern` |
  | Single structural slot         | `<slotname>.<kind>`   | `/main.stack/input.pattern`  |
  | Indexed structural slot        | `<slotname>_<i>.<kind>` | `/main.stack/effects_0.effect` |
- **Parsing error model:** `NodePath::parse(s) -> Result<NodePath,
  PathParseError>`. Errors are typed (
  `EmptySegment | TrailingSeparator | BadName | BadKind |
  TooDeep(usize)`). 64-segment depth cap (defensive against
  malformed TOML).

### `PropPath`

```
prop_path  := segment ("." segment | "[" index "]")*
segment    := name_part
index      := uint
```

### Decisions

- **Already exists in `lpc-model::types::prop_path`.** Only
  decision: **dot for fields, brackets for indices**.
  `params.gradient`, `params.colors[0]`, `state.frame_count`,
  `inputs[0]`, `outputs[0].rgba_buffer`. Confirmed (Q-C resolution).
- **Namespace is the leading segment**: `params.<name>`,
  `inputs[<i>]`, `outputs[<i>]`, `state.<name>`. Resolves the
  "how does the editor know which namespace" question: the editor
  always passes a fully-qualified `PropPath` starting with the
  namespace name.

## §7 — Children from two sources

Both **structural** children (an `Effect` in a `Stack.effects[i]`)
and **param-promoted** children (a `Pattern` filling a
`Kind::Gradient` param) end up as ordered `Vec<Uid>` on the parent.

### Decisions

- **`ChildKind` discriminator:**
  ```rust
  pub enum ChildKind {
      Structural { source: SlotIdx },     // inputs[N]
      ParamPromoted { source: Name },     // params.<name>
  }
  ```
  Stored alongside `children` on `NodeEntry`. Tree iteration is
  unaware of the distinction; editor / sync layer can filter on it.
- **Internal-vs-external surfacing:** Borrowing Godot's
  `INTERNAL_MODE_FRONT/BACK` (prior-art §7). Param-promoted
  children are flagged "internal": the structural children list
  visible to editor consumers excludes them by default; render
  traversal includes them; debug views show them with a marker.
- **Lifecycle binding:**
  - **Structural child** lifetime = parent's lifetime.
    `add_input(stack, effect)` adds; `remove_input(stack, idx)`
    destroys the subtree.
  - **Param-promoted child** lifetime = slot binding's lifetime.
    `set_property(params.gradient, Pattern("fluid"))` instantiates
    the child; `set_property(params.gradient, ColorRamp(...))`
    destroys the previous child.
- **Path segment** as in §6: structural = `<slotname>_<i>.<kind>`,
  param-promoted = `<paramname>.<kind>`.
- **No re-parenting in v0.** Add or destroy only; no `move_node`.
  Add when M5 surfaces a need.
- **Ordering** (within `children`): structural first (in slot
  order), then param-promoted (in declaration order). Render
  traversal orders by `child_kinds[i]` (structural first), then
  `param_promoted` order — but since render is demand-driven via
  outputs, this only matters for explicit subtree iteration.

## §8 — Sync layer surface

Per F-1 + prior-art §5: **no external prior art**. We design from
lp-engine's existing implementation, polishing the seams.

### Decisions

- **`get_changes(since: FrameId, spec: &ApiNodeSpecifier,
  theoretical_fps: Option<f32>) -> Result<D::Response, Error>`.**
  Domain-parametric. Returns the domain-specific response payload.
  Legacy domain returns `lpl_model::ProjectResponse` (today's
  behaviour); future visual domain returns its own response. The
  generic `ProjectRuntime<D>` only knows the trait bound.
- **Wire envelope:** `Message<R>`, `ServerMessage<R>`,
  `ServerMsgBody<R>` (already generic, M2-pre-unbake). Pin
  `R = D::Response` per consumer.
- **`ApiNodeSpecifier`** (existing): a "watch detail for these
  nodes" hint from client. Stays in `lpc-model` (already does);
  the response payload it shapes is per-domain.
- **Frame versioning per-entry** (§5) drives diffing: client sends
  `since_frame`, server walks `nodes` returning entries where any
  `*_ver > since`. No tree walk on every poll — the tree maintains
  a `dirty_set: HashSet<Uid>` updated on each version bump,
  cleared after each `get_changes`.
- **No protocol-level versioning beyond what's there today.**
  Future protocol bumps go through the existing `Message<R>`
  enumeration.

## §9 — Legacy node mapping

Walks each of `Texture`, `Shader`, `Output`, `Fixture` through the
proposed trait surface. The job is to confirm M5 can port them
without changing `Node`.

### Texture

| Behaviour today                           | New shape                          |
|-------------------------------------------|------------------------------------|
| `TextureConfig { width, height, format }` | `params: { width, height, format }` |
| `TextureRuntime` owns `LpTexture`         | `state: { texture_handle }`         |
| Re-render on demand (lazy)                | `outputs[0]: Slot<Kind::TextureRgba8>`; `D::tick` calls render lazily |
| Renders into its own buffer               | impl `render`; output slot exposes the buffer |
| Reload on `node.json` change              | `update_config`                     |
| No fs-change handler                      | `handle_fs_change` returns `Ok(())` |
| No children                               | `inputs: []`                        |

**Maps cleanly.** No trait change needed.

### Shader

| Behaviour today                                    | New shape                                                                |
|----------------------------------------------------|--------------------------------------------------------------------------|
| `ShaderConfig { glsl_path, target_textures, ... }` | `params: { glsl_path, target_textures }`                                  |
| Reads `main.glsl`                                  | `handle_fs_change` watches `*.glsl`                                       |
| Compiles GLSL → LPVM → native                       | `init` (or first `render` on lazy)                                        |
| `shed_optional_buffers` drops compiled output      | `shed_optional_buffers`                                                   |
| Renders into target textures                       | `inputs: [target_texture_refs]` (param-promoted? structural? — see below) |
| Frame-time logging via `time_provider`             | Stays; lives on `ProjectRuntime`, not `Node`                              |

**Edge case:** `target_textures` are Uid references to other
nodes. Today this is a path-string in the config, resolved at init.
In the new shape: it's a **bus binding** (`Binding::Bus(channel)`)
on a slot in `inputs`. The texture nodes publish to a channel; the
shader subscribes. M5 introduces the bus stub for this; it's the
first real bus binding the spine has to carry, predating the lp-vis
roadmap's full bus story.

**Maps cleanly with the bus stub.**

### Output

| Behaviour today                          | New shape                                                                        |
|------------------------------------------|----------------------------------------------------------------------------------|
| `OutputConfig { pin, format, channel_count, ... }` | `params: { pin, format, channel_count }`                                |
| `OutputRuntime` owns an `OutputChannelHandle` | `state: { output_handle }`                                                  |
| `init` opens the channel                 | `init`                                                                           |
| `destroy` closes the channel             | `destroy`                                                                        |
| Receives buffer from a Fixture           | `inputs[0]: Slot<Kind::TextureRgba8 | Kind::TextureWs2811>` — bus-bound from the Fixture |
| No `render` (drains the buffer)          | `render` flushes the input buffer to hardware                                    |

**Maps cleanly.** Output is "leaf node, has inputs, no outputs."

### Fixture

| Behaviour today                                        | New shape                                                                                |
|--------------------------------------------------------|------------------------------------------------------------------------------------------|
| `FixtureConfig { mapping, gamma, output_ref, texture_ref }` | `params: { mapping, gamma }`; `inputs[0]: texture (bus)`; `inputs[1]: output (bus)` |
| `FixtureRuntime` owns `Mapping` data                   | `state: { mapping }`                                                                     |
| Samples the texture, writes to the output buffer       | `render`                                                                                 |
| `handle_fs_change` for `mapping.json`                  | `handle_fs_change`                                                                       |

**Maps cleanly.**

### Mapping summary

All four legacy nodes map without changes to `Node`. The two new
constraints they reveal (and are accommodated by the design):

1. **Bus-binding stub** in M5 for `target_textures` /
   `output_ref` / `texture_ref`. Just a stub — full bus design is
   the lp-vis roadmap's job. Rate-limited to "channel name resolves
   to a node Uid via a flat map."
2. **Lazy render order** is per-domain (`D::tick`), not in the spine.
   Legacy domain implements lp-engine's `ensure_texture_rendered`
   walker against the new tree-based `NodeTree`.

## §10 — Open questions deferred to M4 / M5

These are intentionally deferred — not because they're unanswered,
but because answering them now risks dictating implementation detail
that M4's `/plan` should own.

- **`SlotView` vs concrete `&SlotMap`.** The trait-objected accessor
  in §1 was chosen for crate isolation (`lpc-runtime` doesn't need
  to know `SlotMap`'s exact layout). M4 may inline the type if it's
  faster and `lpc-model` exports the type publicly anyway.
- **Sidecar `state` representation.** `&dyn SidecarState` is a
  placeholder. Choice between (a) a typed enum the domain provides,
  (b) `serde_json::Value`-equivalent for opacity, (c) a per-node
  `Box<dyn Any>` is M4 work. Editor only ever queries via
  `NodeProperties::get_property(state.<name>)` — the encoding is
  invisible to the wire.
- **Bus `BindingResolver` shape.** The lp-vis roadmap designs the
  full bus. M5 only stubs (flat `HashMap<ChannelName, Uid>`).
- **`%unique-name` scope.** §6 reserves the syntax. Whether the
  scope is project-root or `Show`-relative is a lp-vis question.
- **`Show` node introduction.** Today's lp-engine has implicit
  project root; the new spine carries the same. lp-vis introduces
  `Show` as a top-level wrapper; M5 leaves this decision open by
  not over-binding `NodePath`'s root semantics.
- **Generational `Uid`.** §2 picks flat. If embedded ever shows
  use-after-free symptoms (it shouldn't — single-thread, slot
  tombstones, immediate refcount-zero eviction), a generational
  upgrade is API-compatible (the `Uid` newtype absorbs the second
  word).

## §11 — Resolution of M2 flags

How this design retires the three flags raised in `notes.md` ("Flags
carried into M3").

### F-M2-1 — `lpc-runtime` depends on `lpl-model`

**Resolved.** `ProjectRuntime` becomes `ProjectRuntime<D:
ProjectDomain>` (§3). The trait `ProjectDomain` carries:

- `type Artifact`
- `type Response`
- `type Config: NodeConfig`
- `node_kind_from_path(path) -> Option<<Self::Artifact as Artifact>::Kind>`
- `instantiate(...)` — replaces `ProjectHooks::init_nodes`
- `tick(...)` — replaces `ProjectHooks::tick`
- `handle_fs_changes(...)` — replaces `ProjectHooks::handle_fs_changes`
- `build_response(...)` — replaces `ProjectHooks::get_changes`

`lpc-runtime` then has zero dependency on `lpl-model`. `lpl-runtime`
provides `LegacyDomain: ProjectDomain` with
`type Response = lpl_model::ProjectResponse` (etc.).

### F-M2-2 — `ProjectHooks` global state

**Resolved.** The `static HOOKS: Mutex<...>` singleton is **deleted**.
`ProjectRuntime<D>` carries `D` as a generic / a stored
`Arc<dyn ProjectDomain>`; consumer code constructs:

```rust
let domain  = Arc::new(LegacyDomain::new(...));
let runtime = ProjectRuntime::new(fs, output, domain);
```

`lpl_runtime::install()` becomes
`lpl_runtime::LegacyDomain::new()` (a constructor, not a side-effecting
function). Tests instantiate `ProjectRuntime::<LegacyDomain>` directly.

The "consumer forgot to install hooks → runtime panic" footgun is
replaced by "consumer must pass `domain` to `ProjectRuntime::new` —
type system catches the mistake at compile time."

### F-M2-3 — Hardcoded `texture / shader / output / fixture`
suffix list in `loader.rs`

**Resolved.** `lpc-runtime/src/project/loader.rs` shrinks to:

- `discover_nodes(fs, project_dir, domain) -> Vec<NodeDir>` — walks
  the fs, calls `domain.node_kind_from_path` to recognise dirs.
- `load_from_filesystem(fs) -> Result<ProjectConfig>` — parses
  `project.json`. Stays domain-agnostic (`ProjectConfig` is generic).

The hardcoded suffix list moves into
`lpl_runtime::LegacyDomain::node_kind_from_path` as a `match` over
the four legacy kinds. Future domains supply their own.

The lpv-runtime roadmap will add `VisualDomain::node_kind_from_path`
that recognises `.pattern`, `.effect`, `.stack`, `.transition`,
`.live`, `.playlist`.

## §12 — Cross-references

- Strawman + decisions log: [`notes.md`](notes.md). M3 didn't
  re-decide any of the resolved items there; the design above
  builds on them.
- Prior-art synthesis: [`prior-art.md`](prior-art.md). Cited
  inline as "(prior-art §N)" where decisions trace to specific
  findings.
- M2 as-built: `lp-core/lpc-model/`, `lp-core/lpc-runtime/`,
  `lp-legacy/lpl-model/`, `lp-legacy/lpl-runtime/`. The current
  `NodeRuntime` trait is the kernel of `Node` (§1); the current
  `ProjectRuntime` is the kernel of `ProjectRuntime<D>` (§3).
- M2 deviation flags: see [`notes.md`](notes.md) "Flags carried
  into M3" and §11 above for the resolutions.

## §13 — Design erratum log

Empty at write time. As M4 / M5 surface mistakes in this doc,
errata land here with the date and the discovery context.
