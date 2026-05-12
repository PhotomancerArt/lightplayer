# 01 — `NodeTree`, `NodeEntry`, lazy lifecycle, child kinds

The tree is the **single source of truth** for parent / children /
status / frame versions. Node impls never have to keep these in sync
with anything; the tree owns them exclusively.

## Identity

```rust
pub struct NodeId(pub u32);             // runtime; opaque; never authored
pub struct NodePath(pub Vec<NodePathSegment>);
pub struct NodePathSegment {
    pub name: NodeName,                 // instance name in parent
    pub ty:   NodeName,                 // kind tag, e.g. "show" or "pattern"
}
```

- `NodeId` is the cheap `Copy` runtime handle (`u32`, no
  generational bits — embedded scale doesn't justify the cost; if
  use-after-free symptoms ever appear, the newtype absorbs a second
  word with no API break).
- `NodePath` is the **persistent identity** for authored content.
  Reconstruction (e.g., a Pattern reappears on disk) gets a *new*
  `NodeId` and re-resolves the path; no id reuse.
- Slash-leading, never trailing: `/main.show/fluid.pattern`.
- **Sibling-name uniqueness** is enforced at add-child time:
  `add_child(parent, name, ty)` returns `Err(SiblingNameCollision)`
  if `(parent, name)` already exists.
- **The path is the wire-stable identity** the editor uses to
  navigate; `NodeId` is process-local and must never appear in
  authored TOML.

## `NodeTree` container

```rust
pub struct NodeTree<D: ProjectDomain> {
    nodes: Vec<Option<NodeEntry<D>>>,                 // indexed by NodeId.0
    next_id: u32,
    by_path: HashMap<NodePath, NodeId>,                // O(1) path lookup
    by_sibling: HashMap<(NodeId, NodeName), NodeId>,   // sibling-uniqueness index
    root: NodeId,
}
```

- **Flat `Vec<Option<NodeEntry>>` indexed by `NodeId.0`** (per
  prior-art §2 "O(1) HashMap for child-by-name lookup", plus F-2
  performance). Tombstones (`None`) on destroy; `next_id` monotonic,
  no reuse.
- **Tombstones over compaction.** Destroy sets the slot to `None`;
  the slot stays. No id reuse, no generational handling — the path
  is the persistent identity.
- **Persistence is paths, not ids.** TOML references children by
  `NodePath`; the runtime resolves on load.

## `NodeEntry`

```rust
pub struct NodeEntry<D: ProjectDomain> {
    pub id: NodeId,
    pub path: NodePath,                                 // canonical absolute
    pub parent: Option<NodeId>,
    pub child_kind: Option<ChildKind>,                  // None for root; immutable for entry's lifetime
    pub children: Vec<NodeId>,                          // ordered

    pub status: NodeStatus,                             // Created | Ok | Warn | Error | InitError
    pub state: EntryState,                              // Pending | Alive(Box<dyn Node>) | Failed

    // Three frame counters per entry (12 bytes/entry); see "Frame versioning" below.
    pub created_frame: FrameId,                         // set on insert; never bumped
    pub change_frame:  FrameId,                         // bumped on status / EntryState / NodeConfig change
    pub children_ver:  FrameId,                         // bumped on children-list mutation

    pub config:   NodeConfig,                           // authored use-site data (§04)
    pub artifact: ArtifactRef<D::Artifact>,             // refcount holder (§03)

    pub prop_cache: BTreeMap<PropPath, ResolvedSlot>,   // §06; future: separate prop_cache_ver if editor watches live state
}
```

### What lives where

| Lives on `Node` (the impl)                                            | Lives on `NodeEntry` (the container)                                                |
|-----------------------------------------------------------------------|-------------------------------------------------------------------------------------|
| (no identity accessors)                                               | identity (`id`, `path`, `parent`) — **source of truth**; passed via context         |
| (no parent / children accessors)                                      | `parent`, `children`, `child_kind` — exclusive owner                                |
| `*Props` (outputs + state only)                                       | nothing (the impl owns its `*Props`)                                                |
| `props() -> &dyn PropAccess` accessor                                 | nothing                                                                             |
| lifecycle hooks (`tick` / `destroy` / `handle_memory_pressure`)       | `status`, `created_frame` / `change_frame` / `children_ver`, `EntryState`           |
| (impl exists only when `Alive`)                                       | `EntryState::{Pending,Alive,Failed}` — the lazy lifecycle owner                     |

The trait carries **no tree links**. `Node` impls do not know their
own `NodeId`, parent, or children. ([02](02-node.md))

## `EntryState` — always-lazy lifecycle

```rust
pub enum EntryState {
    /// Artifact handle resolved + refcounted; node not instantiated.
    Pending,
    /// Node instantiated and ticking.
    Alive(Box<dyn Node>),
    /// Instantiation failed; resolution falls through to slot.default.
    Failed(ErrorReason),
}
```

Children are **always-lazy by default**. Memory pressure on ESP32
is the dominating constraint, and "user has 30 sidecars in their
library, uses 4 at a time" is a real authoring shape. Eager
instantiation would be a footgun.

### Init pass for a parent

When the parent is woken (or loaded), for each child it:

1. Resolves the `ArtifactSpec` via `ArtifactManager`. This
   transitions the artifact `Resolved → Loaded` (parse + schema
   validate, [03](03-artifact.md)), increments the refcount, and
   stores the `ArtifactRef`.
2. Creates a `NodeEntry` with `EntryState::Pending` and registers
   it in the tree (it has a `NodeId`, `NodePath`, `parent`,
   `ChildKind`, the resolved artifact handle, and the per-instance
   `NodeConfig`).
3. **Stops there.** No `Box<dyn Node>` until the entry is woken.

### Two error tiers

- **Parse-time** (artifact-level, surfaces at parent-init).
  Path-not-found, TOML schema error, type error. The entry never
  reaches `Pending`; the parent's load returns `Err` and the user
  sees the failure immediately.
- **Init-time** (node-level, surfaces lazily on first wake).
  Shader compile failure, OOM during resource allocation, etc.
  Entry transitions `Pending → Failed`. Resolution treats `Failed`
  like an empty channel — falls through to `Slot.default`. The
  `NodeEntry` records the reason; editor surfaces it. Optional
  retry on next `config_ver` bump or memory-pressure release
  sequence.

### Wake trigger (M5; will iterate)

First implementation: **wake on demand from binding resolution**.
When a slot's binding lands on `NodeProp { node: <pending child> }`,
the resolver transitions the child's entry `Pending → Alive` by
invoking `D::instantiate(artifact, config, ctx)` synchronously,
mid-tick.

- **Risk**: `D::instantiate` may include shader compilation (= JIT,
  = real time). Mid-tick is the hot path; this *will* introduce
  spikes for first-touch shaders.
- **Refinement plan**: a pre-tick warmup pass that walks the
  binding graph and wakes any reachable `Pending` children before
  `tick` starts. Cheap if cached: only changed bindings produce
  new wake-ups. Land it when measurement justifies it.
- **Out-of-band wake** (editor opens a node's detail view) is
  deferred until editor flows demand it.

### Memory-pressure interaction

A natural top-of-pressure response is to **demote** the
most-recently-unused `Alive` Sidecar back to `Pending`: call
`Node::destroy` on it, drop the box, keep the entry. Next access
re-instantiates via `D::instantiate`. The release valve falls out
of the lazy model for free.

This is **distinct** from `Node::handle_memory_pressure`, which
keeps the node alive and asks it to shed reconstructable buffers.
The engine reaches for `handle_memory_pressure` first; demotion
is a bigger hammer.

### Cycles

`A` binds to `B.outputs[0]`, `B` binds to `A.outputs[0]`: detected
during binding-resolution build-up, reported as a config error.
M5 forbids cycles outright.

Future direction: read **last frame's value** for backward edges in
a detected cycle — supports feedback (an input bound to its own
output, with one-frame delay) elegantly, at the cost of an implicit
per-cycle-edge buffer of last-frame values. Defer until a real
feedback use case demands it.

## `ChildKind` — three child kinds, three lifetimes

Every child has a `ChildKind` discriminator. This determines **what
TOML form authored it** and **when it gets destroyed**. All three
are fully realised `NodeEntry`s: addressable by `NodePath`,
bindable from anywhere via `NodeProp { node, prop }`, and walked
identically by tick.

| ChildKind | Authored as                              | Lifetime                         | Use case                                                      |
|-----------|------------------------------------------|----------------------------------|---------------------------------------------------------------|
| `Input`   | `[input] visual = "..."` (etc.)          | parent's lifetime                | structural composition (Stack `[input]` and `[[effects]]`)    |
| `Sidecar` | `[children.<name>]`                      | parent's lifetime                | shared LFOs, programmer model: one node, many bindings to it  |
| `Inline`  | `[params.<name>.bind] visual = "..."`    | the slot binding's lifetime      | author UX: drop a Visual on a slot in one operation           |

```rust
pub enum ChildKind {
    Input    { source: WireSlotIndex },     // [input.N]
    Sidecar  { name: NodeName },      // [children.<name>]
    Inline   { source: PropPath },    // [params.<name>.bind] visual = ...
}
```

### Where each kind comes from

- **`Input`** and **`Sidecar`** are declared by the *artifact*. When
  a `Stack` artifact says `[input] visual = "fbm.pattern.toml"`,
  every Stack instance gets an `Input` child. The artifact owns
  structural composition.
- **`Inline`** is declared by the *parent's `NodeConfig`*. When a
  parent's authored TOML overrides a slot with
  `[params.gradient.bind] visual = "../fluid.pattern.toml"`, the
  loader desugars it into (a) creating an `Inline` child on the
  parent + (b) installing a `Binding::NodeProp { node: <child path>,
  prop: outputs[0] }` on the slot. The runtime never sees the
  `visual = "..."` form — only the desugared pair.

### Lifetime rules

- **Removing the last binding to a `Sidecar`** does *nothing* —
  Sidecars are parent-owned, not binding-owned. That matches the
  shared-LFO use case.
- **Removing or changing an `Inline`'s authoring binding** destroys
  the child by definition. If the binding switches from `visual =
  "..."` to `literal = ...`, the child is destroyed. If the new
  binding is also `visual = "..."` for a different artifact, the
  old child is destroyed and a new one created.
- **Sharing across slots is `Sidecar`-only.** `Inline` is 1:1 with
  its slot by construction. Trying to bind a different slot to an
  `Inline` child would imply two lifecycle owners — flag as
  authoring error.

### Lifecycle ordering

- **Tree-init**: depth-first, children-first. For lazy children,
  "init" means *create a Pending entry and validate the artifact*
  — not call `D::instantiate`.
- **Wake** (`Pending → Alive`): bottom-up. Descendants of the woken
  node are woken before the node itself runs `D::instantiate`. A
  parent's `instantiate` may assume all of its descendants are
  `Alive`.
- **Destroy**: depth-first, children-first. Parents observe a clean
  teardown after their subtree is gone. (A future top-down
  `pre_destroy` hook may be added if a real consumer needs it; see
  [02 §1.Y](02-node.md).)
- **Demote** (`Alive → Pending` under memory pressure): top-down.
  A demoted node's children may still be `Alive` if they're
  referenced from outside the demoted subtree (typical for
  `Sidecar` children).

## `NodeStatus` (existing in M2)

```rust
pub enum NodeStatus {
    Created,
    InitError(String),
    Ok,
    Warn(String),
    Error(String),
}
```

Already shipped in M2's `lpc_model::project::api::NodeStatus`.
Stays as-is; the spine writes to it on lifecycle transitions.
String payloads are TOML-friendly + cheap (load-bearing F-1).

## Why a tree (and not a graph)

Authored structure is hierarchical: a Show contains Visuals; a Stack
contains an Input and Effects; a Pattern contains Params. Nodes
have a single parent.

Cross-tree references (a Pattern bound to an LFO somewhere else in
the show) are **bindings**, not edges in the graph. A `Sidecar` LFO
lives in the tree once; many bindings can point at its outputs.
This keeps lifecycle simple (one parent → one destroyer) while
preserving expressivity at the data flow layer.

## Frame versioning

Per-entry counters (three for M5; ~12 bytes/entry):

- `created_frame` — set when the entry is first inserted; never
  bumped after. Lets a client distinguish "entry I haven't seen
  yet" from "entry that changed since I last looked".
- `change_frame` — bumped on `status` change, on `EntryState`
  transition (`Pending → Alive → Failed`), and on `NodeConfig`
  edit (set_property, binding edit, hot reload of the parent's
  TOML). One coarse counter is enough for M5; the editor doesn't
  yet need to distinguish "status flipped" from "config edited".
- `children_ver` — bumped on any children-list mutation
  (insert, remove, reorder). Drives `TreeDelta::ChildrenChanged`
  ([07](07-sync.md)). The client diffs the children list against
  its mirror to **infer** removals — the server never tracks
  destroyed ids.

The sync layer reads `since > FrameId` to find dirty entries
([07](07-sync.md)).

**Why three (not five).** Earlier drafts had separate `status_ver`,
`config_ver`, `state_ver`. M5 collapses status/state/config into
`change_frame` because the editor's first-pass UX doesn't need
finer granularity, and every `FrameId` field is 4 bytes/entry on
the hot path. When the editor grows live-state watching, a
separate `prop_cache_ver` (or per-prop versioning inside
`prop_cache`) is the natural extension — kept commented in
`NodeEntry` so the future shape is visible at the call site.

## Open questions

- **Generational `NodeId`.** Picking flat for M5. If embedded ever
  shows use-after-free symptoms (it shouldn't — single-thread, slot
  tombstones, immediate refcount-zero eviction), generational
  upgrade is API-compatible.
- **`%unique-name` scope** (legacy bridge convenience syntax).
  Whether the scope is project-root or `Show`-relative is a lp-vis
  question; M5 doesn't bind it.
- **`Show` node introduction.** Today's lp-engine has implicit
  project root; the new spine carries the same. lp-vis introduces
  `Show` as a top-level wrapper; M5 leaves this open by not
  over-binding `NodePath`'s root semantics.
