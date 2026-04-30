# M3 — Tree spine implementation

> **Naming (planning onward):** `lpc-runtime` in this M3 plan denotes
> **`lpc-engine`** after M4.3a. Structural delta types migrated to **`lpc-wire`**
> as `WireTreeDelta` (conceptually aligned with `TreeDelta` here).

Stand up the new node-tree types — `NodeTree`, `NodeEntry`,
`EntryState`, `ChildKind`, the wire-side mirror, and the structural
delta protocol — across `lpc-model`, `lpc-engine` (still called `lpc-runtime` when M3 was written),
and `lp-engine-client`. The new types **coexist** with the legacy
`ProjectRuntime` flat-map; cutover lands in a later plan.

Reference: [`../design/01-tree.md`](../design/01-tree.md) and
[`../design/07-sync.md`](../design/07-sync.md).

# Notes

## Scope of work

**In scope:**

- `lpc-model`:
  - `ChildKind` enum (`Input { source: SlotIdx } | Sidecar { name } |
    Inline { source: PropPath }`) with serde.
  - `SlotIdx(pub u32)` placeholder newtype.
  - `EntryStateView` enum (`Pending | Alive | Failed { reason }`)
    — the wire-shape discriminant, no payload.
  - `TreeDelta` enum — domain-agnostic structural deltas, three
    variants: `Created`, `EntryChanged`, `ChildrenChanged`. **No
    `Destroyed`** — clients infer removals by diffing the new
    children list against their mirror.
  - Existing `NodeId`, `NodePath`, `NodePathSegment`, `NodeStatus`,
    `FrameId`, `PropPath` reused as-is (already shipped in M2).
- `lpc-runtime`:
  - `NodeTree<N>` container — `Vec<Option<NodeEntry<N>>>` indexed by
    `NodeId.0`, `BTreeMap<NodePath, NodeId>` path index,
    `BTreeMap<(NodeId, NodeName), NodeId>` sibling-uniqueness index,
    `next_id` monotonic counter, root.
  - `NodeEntry<N>` — `id`, `path`, `parent`, `child_kind: Option<ChildKind>`,
    `children`, `status`, three frame counters
    (`created_frame`, `change_frame`, `children_ver`),
    `state: EntryState<N>`. Future fields (`config`, `artifact`,
    `prop_cache`, `prop_cache_ver`) **kept as commented-out stubs
    in the source** so the destination shape is visible at the
    call site.
  - `EntryState<N>` — server-side enum: `Pending | Alive(N) |
    Failed(ErrorReason)`. `N` is the impl payload type; in this
    plan we use `N = ()` (no Node trait yet).
  - Mutation API: `add_child(parent, name, ty, child_kind, …) ->
    Result<NodeId, TreeError>`, `remove_subtree(id)`, `lookup_path`,
    `lookup_sibling`, `get`, `get_mut`, `entries()`, frame-versioned
    `tree_deltas_since(since: FrameId) -> Vec<TreeDelta>`.
- `lp-engine-client`:
  - `ClientNodeTree` — mirror with `BTreeMap<NodeId, ClientTreeEntry>`
    + `BTreeMap<NodePath, NodeId>` index.
  - `ClientTreeEntry` (named distinctly to avoid collision with
    the legacy `ClientNodeEntry` in `project::view`) — `id`,
    `path`, `parent`, `child_kind`, `children`, `state:
    EntryStateView`, `status`, three frame counters. Future
    `prop_cache` / `prop_cache_ver` left as commented stubs.
  - `apply_tree_delta(delta: &TreeDelta)` per-delta application,
    inferring removals on `ChildrenChanged` by diffing against the
    current mirror.
  - Stays a separate type from existing `ClientProjectView` for now;
    they coexist.
- Tests covering: add/remove, path/sibling indexing, depth-first
  destroy, tombstone behaviour (no id reuse), serde round-trip on
  every wire-side type, full tree → deltas → mirror parity round-trip
  including inferred-removal correctness on `ChildrenChanged`.

**Out of scope (explicitly):**

- The `Node` trait surface. `EntryState::Alive(N)` is generic;
  this plan instantiates with `N = ()`. (Source carries a
  `// later: N = Box<dyn Node>` comment.)
- `NodeConfig` on `NodeEntry` (separate plan; commented stub in
  source).
- `ArtifactRef`, `ArtifactManager` (separate plan; commented stub).
- `prop_cache` on `NodeEntry` and `ClientNodeEntry` (commented
  stub in source).
- Per-prop deltas / `prop_cache_ver` field on `NodeEntry`
  (commented stub in source).
- Wiring into legacy `ProjectRuntime`; existing flat-map stays
  authoritative.
- Generic `ProjectDomain` trait. `NodeTree<N>` is the only
  parameterisation in this plan.
- Per-prop sync deltas (Props/PropAccess work).
- Bus / binding resolution.
- The `Inline` desugaring / cascade-binding materialisation.
- The actual fs-watch routing into the new tree.
- `pre_destroy` hook (not in any version of the design).

## Current state

- M2 ships `NodeId`, `NodePath`, `NodePathSegment`, `NodeName`,
  `PathError` in `lpc-model::node` (post-cleanup).
- M2 ships `NodeStatus` in `lpc-model::project::api`.
- M2 ships `FrameId` in `lpc-model::project`.
- `Prop<T>` lives in `lpc-model::prop::prop` (renamed from
  StateField).
- Legacy `ProjectRuntime` (`lpc-runtime::project::project_runtime`)
  has a flat `BTreeMap<NodeId, NodeEntry>` where `NodeEntry`
  carries `path, kind, config, config_ver, status, status_ver,
  runtime, state_ver`. **No tree. No EntryState. No ChildKind.**
- Legacy `ClientProjectView` (`lp-engine-client::project::view`)
  is the legacy mirror; flat-map, hard-coded to legacy NodeKind.
  **Does not get touched in this plan** — the new
  `ClientNodeTree` lives alongside.
- `lpc-runtime::nodes::NodeRuntime` is the *legacy* per-node trait
  (`init`, `update_config`, `render`, `destroy`, etc.). The new
  `Node` trait lands in a later plan.

## Resolved decisions

| #   | Decision                                                                                                                |
|-----|-------------------------------------------------------------------------------------------------------------------------|
| Q1  | **Plan lives in roadmap.** `docs/roadmaps/2026-04-28-node-runtime/m3-tree-spine-impl/plan.md`. Stays in place when done. |
| Q2  | **`EntryState<N>` is generic; `N = ()` in this plan.** Node-trait plan later swaps to `N = Box<dyn Node>`.              |
| Q3  | **`NodeTree` is NOT wired into legacy `ProjectRuntime`.** New `lpc-runtime::tree` module; coexists.                     |
| Q4  | **Skip `config` / `artifact` / `resolver_cache` on `NodeEntry`.** TODO pinpoints; separate plans.                        |
| Q5  | **Tree wire deltas are domain-agnostic.** No `kind` / `config` payload on `Created`.                                    |
| Q6  | **`Vec<Option<NodeEntry>>` indexed by `NodeId.0`.** Tombstones; no id reuse.                                            |
| Q7  | **`BTreeMap` for `by_path` and `by_sibling`.** No new dep. Hashbrown stays an option for later if profiling justifies.   |
| Q8  | **`ChildKind::Input { source: SlotIdx }` with `pub struct SlotIdx(pub u32)` placeholder** in lpc-model. Revisit with slots. |
| Q9  | **`ChildKind::Inline { source: PropPath }`** reuses existing `lpc-model::PropPath`.                                     |
| Q10 | **Tests in each crate's existing test layout.** No integration crate.                                                    |
| Q11 | **`TreeDelta` lives in `lpc-model::tree::*`.** Not in `project::api` (legacy-flavoured).                                |
| Q12 | **`ClientNodeTree` lives in `lp-engine-client::tree`.** Legacy `project::view` is left alone.                            |
| Q13 | **(c) — coexistence.** `NodeTree::tree_deltas_since` exists and is unit-tested. Legacy `ProjectRuntime` keeps shipping  |
|     | its existing `ProjectResponse`. End-to-end "server tree → wire → client tree mirror" is exercised inside `lpc-runtime` + |
|     | `lp-engine-client` without touching `ProjectRuntime` or `lp-server`. Cutover is later work.                              |
| Q14 | **`child_kind` on the child entry, not parallel arrays on the parent.** `NodeEntry::child_kind: Option<ChildKind>`;     |
|     | `None` for root, immutable for the entry's lifetime. Cleaner invariants, simpler `Created` delta payload.                |
| Q15 | **Three frame counters per entry.** `created_frame` (set on insert; never bumped), `change_frame` (status / state /     |
|     | future config), `children_ver` (children-list mutation). ~12 bytes/entry. Future `prop_cache_ver` kept as commented      |
|     | stub in source.                                                                                                          |
| Q16 | **`TreeDelta` has 3 variants, no `Destroyed`.** `Created`, `EntryChanged { id, status, state, change_frame }`,          |
|     | `ChildrenChanged { id, children, children_ver }`. Client diffs the new children list against its mirror to **infer**    |
|     | removals; server never tracks destroyed ids.                                                                             |
| Q17 | **Future fields stay as commented-out stubs in source.** `NodeEntry::config`, `NodeEntry::artifact`, `prop_cache`,       |
|     | `prop_cache_ver`, `EntryState`'s future `N = Box<dyn Node>`, `TreeDelta::Created.config`, `EntryChanged.config`,         |
|     | future `PropsChanged` variant. Visible at the call site so the destination shape is in mind during M3 review and the     |
|     | follow-on plans uncomment + fill them in.                                                                                |

# Design

## File structure

New modules slot into the existing layout — every crate already has
the parent module the new files attach to.

### `lpc-model` (shared types)

Existing `lp-core/lpc-model/src/tree/mod.rs` is empty; this plan
populates it. New files:

- `lp-core/lpc-model/src/tree/mod.rs` — re-exports.
- `lp-core/lpc-model/src/tree/child_kind.rs` — `ChildKind` enum
  (`Input { source: SlotIdx } | Sidecar { name } | Inline { source }`).
- `lp-core/lpc-model/src/tree/slot_idx.rs` — `SlotIdx(pub u32)`
  placeholder (the "real" slot indexing lands with `Slot` /
  artifact schema work; we only need the type to exist for
  `ChildKind` to be expressible).
- `lp-core/lpc-model/src/tree/entry_state_view.rs` — wire-shape
  `EntryStateView` (`Pending | Alive | Failed { reason }`).
- `lp-core/lpc-model/src/tree/tree_delta.rs` — `TreeDelta` enum
  with three variants. Future-extension comments on
  `Created.config` / `EntryChanged.config` / future
  `PropsChanged`.

`lpc-model::lib.rs` re-exports `tree::{ChildKind, SlotIdx,
EntryStateView, TreeDelta}` at the crate root.

### `lpc-runtime` (server-side tree)

New module `tree` next to `project`:

- `lp-core/lpc-runtime/src/tree/mod.rs` — re-exports + module
  doc tying back to `design/01-tree.md`.
- `lp-core/lpc-runtime/src/tree/node_entry.rs` — `NodeEntry<N>`
  with `child_kind: Option<ChildKind>` + the three frame
  counters. Commented-out stubs for `config`, `artifact`,
  `prop_cache`, `prop_cache_ver`.
- `lp-core/lpc-runtime/src/tree/entry_state.rs` — server-side
  `EntryState<N>` with `Pending | Alive(N) | Failed(reason)`.
  Comment notes future `N = Box<dyn Node>` substitution.
- `lp-core/lpc-runtime/src/tree/node_tree.rs` — `NodeTree<N>`
  container: `Vec<Option<NodeEntry<N>>>`, two `BTreeMap`
  indices, mutation API.
- `lp-core/lpc-runtime/src/tree/tree_error.rs` — `TreeError`
  enum (`SiblingNameCollision { parent, name }`,
  `UnknownNode(NodeId)`, `UnknownPath(NodePath)`,
  `RootMutation`, etc.).
- `lp-core/lpc-runtime/src/tree/sync.rs` — `tree_deltas_since`
  walker (server-side delta generation).

`lpc-runtime::lib.rs` adds `pub mod tree;` and re-exports
`tree::{NodeTree, NodeEntry, EntryState, TreeError}`.

`ProjectRuntime` is **not** modified.

### `lp-engine-client` (client-side mirror)

New module alongside the existing `project::view`:

- `lp-core/lp-engine-client/src/tree/mod.rs` — re-exports.
- `lp-core/lp-engine-client/src/tree/client_node_tree.rs` —
  `ClientNodeTree` with `BTreeMap<NodeId, ClientTreeEntry>` +
  `BTreeMap<NodePath, NodeId>` index + `last_synced_frame`.
- `lp-core/lp-engine-client/src/tree/client_tree_entry.rs` —
  `ClientTreeEntry` (named distinctly to avoid collision with
  the existing legacy `ClientNodeEntry` in
  `project::view`). Mirror of `NodeEntry<()>` minus impl
  payload.
- `lp-core/lp-engine-client/src/tree/apply.rs` —
  `apply_tree_delta(&mut ClientNodeTree, &TreeDelta)`,
  including the inferred-removal logic on
  `ChildrenChanged`.

`lp-engine-client::lib.rs` adds `pub mod tree;` and re-exports
`tree::{ClientNodeTree, ClientTreeEntry}`.

`ClientProjectView` and the legacy `ClientNodeEntry` stay
untouched.

## Architecture (M3 slice)

```
┌────────────────────────── lpc-model ──────────────────────────┐
│   tree::{ChildKind, SlotIdx, EntryStateView, TreeDelta}       │
│   (existing) NodeId, NodePath, NodeStatus, FrameId, PropPath  │
└────────────────┬─────────────────────────────────┬────────────┘
                 │                                 │
                 │ shared types                    │ shared types
                 ▼                                 ▼
┌────────────── lpc-runtime ─────────────┐  ┌── lp-engine-client ───┐
│ tree::NodeTree<N> (server, N = ())     │  │ tree::ClientNodeTree   │
│ tree::NodeEntry<N>                     │  │ tree::ClientTreeEntry  │
│ tree::EntryState<N>                    │  │ tree::apply_tree_delta │
│ tree::tree_deltas_since() ─────────────┼──┼─► consumes &TreeDelta  │
│                                        │  │   (in tests within     │
│ legacy ProjectRuntime stays as-is      │  │    these crates)       │
└────────────────────────────────────────┘  └────────────────────────┘

         (legacy wire path stays separate this milestone)
```

The server-tree → wire → client-tree round-trip is exercised in
test code that lives in `lpc-runtime` and `lp-engine-client`. No
production wire path consumes `TreeDelta` yet; that's a future
plan.

## Concrete shapes (with commented-out future fields)

The future-field comments in source are non-trivial — the user
asked for them explicitly so the destination shape is in mind
during M3 review. The exact sketches:

```rust
// lpc-model::tree::child_kind
pub enum ChildKind {
    Input   { source: SlotIdx },
    Sidecar { name: NodeName },
    Inline  { source: PropPath },
}
```

```rust
// lpc-model::tree::tree_delta
pub enum TreeDelta {
    Created {
        id: NodeId,
        path: NodePath,
        parent: Option<NodeId>,
        child_kind: Option<ChildKind>,
        status: NodeStatus,
        state:  EntryStateView,
        created_frame: FrameId,
        change_frame:  FrameId,
        children_ver:  FrameId,
        // Coming soon:
        // config: NodeConfig,
    },
    EntryChanged {
        id: NodeId,
        status: NodeStatus,
        state:  EntryStateView,
        change_frame: FrameId,
        // Coming soon:
        // config: Option<NodeConfig>,
    },
    ChildrenChanged {
        id: NodeId,
        children: Vec<NodeId>,
        children_ver: FrameId,
    },
    // No `Destroyed` — clients infer removals by diffing
    // `children` against their mirror.

    // Coming soon (per-prop deltas; wired when editor demands
    // live-state watching):
    // PropsChanged {
    //     id: NodeId,
    //     entries: Vec<(PropPath, LpsValue)>,
    //     prop_cache_ver: FrameId,
    // },
}
```

```rust
// lpc-runtime::tree::node_entry
pub struct NodeEntry<N> {
    pub id: NodeId,
    pub path: NodePath,
    pub parent: Option<NodeId>,
    pub child_kind: Option<ChildKind>,    // None for root; immutable
    pub children: Vec<NodeId>,            // ordered

    pub status: NodeStatus,
    pub state:  EntryState<N>,

    // Three frame counters per entry. See design/01-tree.md
    // "Frame versioning" for why three (not five).
    pub created_frame: FrameId,           // set on insert; never bumped
    pub change_frame:  FrameId,           // bumped on status / state change
    pub children_ver:  FrameId,           // bumped on children-list mutation

    // Coming soon (separate plans uncomment + fill in):
    // pub config:   NodeConfig,                          // §design/04
    // pub artifact: ArtifactRef,                         // §design/03
    // pub prop_cache: BTreeMap<PropPath, ResolvedSlot>,  // §design/06
    // pub prop_cache_ver: FrameId,                       // when editor watches live state
}
```

```rust
// lpc-runtime::tree::entry_state
pub enum EntryState<N> {
    Pending,
    Alive(N),               // M3: N = (). Later: N = Box<dyn Node>.
    Failed(ErrorReason),
}
```

```rust
// lpc-view::tree::client_tree_entry
pub struct ClientTreeEntry {
    pub id: NodeId,
    pub path: NodePath,
    pub parent: Option<NodeId>,
    pub child_kind: Option<ChildKind>,
    pub children: Vec<NodeId>,

    pub status: NodeStatus,
    pub state:  EntryStateView,

    pub created_frame: FrameId,
    pub change_frame:  FrameId,
    pub children_ver:  FrameId,

    // Coming soon (mirrors NodeEntry future fields):
    // pub config: NodeConfig,
    // pub prop_cache: BTreeMap<PropPath, (LpsValue, FrameId)>,
    // pub prop_cache_ver: FrameId,
}
```

# Phases

The four phases are independently reviewable. Each phase compiles
and tests on its own; the next phase only adds.

## P1 — Wire-shared tree types in `lpc-model`

Stand up the leaf types every other phase depends on. No runtime
behaviour, just data definitions and serde.

- Add files under `lpc-model::tree::*` per Design §file-structure.
- `ChildKind` derives `Debug`, `Clone`, `PartialEq`, `Eq`,
  `Serialize`, `Deserialize`. Variants tagged with serde
  internally for round-trip stability.
- `SlotIdx(pub u32)` — `Debug`, `Clone`, `Copy`, `PartialEq`,
  `Eq`, `Hash`, `Ord`, serde transparent. Module doc points at
  the future slot work.
- `EntryStateView` — same derives.
- `TreeDelta` — same derives. Variants in the order
  `Created` / `EntryChanged` / `ChildrenChanged`. Each variant
  has the commented-out future fields right below its real
  fields.
- Update `lpc-model::lib.rs` to `pub mod tree;` and re-export
  the four public types at the crate root.
- Tests in `lpc-model/src/tree/mod.rs` (or a sibling test module
  per file): serde JSON round-trip for each enum variant /
  struct, including a sample of every `ChildKind` and
  `EntryStateView` discriminant.

Validation: `cargo test -p lpc-model`.

## P2 — Server-side `NodeTree` in `lpc-runtime`

Stand up the container with mutation API + path/sibling
indexing. No deltas yet (that's P3).

- Add files under `lpc-runtime::tree::*` per Design §file-structure.
- `NodeEntry<N>` — fields per Design §concrete-shapes.
  Commented-out future fields land verbatim. Constructor
  `NodeEntry::new(id, path, parent, child_kind, frame)` sets
  `created_frame = change_frame = children_ver = frame`,
  `status = NodeStatus::Created`, `state = EntryState::Pending`,
  `children = Vec::new()`.
- `EntryState<N>` per Design.
- `TreeError` — `SiblingNameCollision { parent: NodeId, name:
  NodeName }`, `UnknownNode(NodeId)`, `UnknownPath(NodePath)`,
  `RootMutation`, `NotInTree(NodeId)`.
- `NodeTree<N>` — fields:
  ```rust
  pub struct NodeTree<N> {
      nodes: Vec<Option<NodeEntry<N>>>,
      by_path: BTreeMap<NodePath, NodeId>,
      by_sibling: BTreeMap<(NodeId, NodeName), NodeId>,
      next_id: u32,
      root: NodeId,
  }
  ```
- API on `NodeTree<N>`:
  - `pub fn new(root_path: NodePath, frame: FrameId) -> Self` —
    inserts a root with `parent = None`, `child_kind = None`.
  - `pub fn add_child(&mut self, parent: NodeId, name: NodeName,
    ty: NodeName, child_kind: ChildKind, frame: FrameId)
    -> Result<NodeId, TreeError>` — inserts entry, registers
    indices, pushes onto parent's `children`, bumps parent's
    `children_ver`.
  - `pub fn remove_subtree(&mut self, id: NodeId, frame: FrameId)
    -> Result<(), TreeError>` — depth-first; tombstones every
    descendant slot, removes them from `by_path` and `by_sibling`,
    pops `id` from its parent's `children`, bumps parent's
    `children_ver`. Forbidden on root.
  - `pub fn get(&self, id: NodeId) -> Option<&NodeEntry<N>>`,
    `get_mut`, `lookup_path`, `lookup_sibling`, `entries()`,
    `entries_mut()`, `root() -> NodeId`.
  - **Important:** `set_status(&mut self, id, status, frame)` and
    `set_state(&mut self, id, state, frame)` bump
    `change_frame = frame`. Direct field mutation through
    `get_mut` is allowed but tests assert that engine-side
    callers always go through the helpers. (The Node-trait plan
    will tighten this further.)
- Update `lpc-runtime::lib.rs` to add `pub mod tree;` and
  re-export the four public types.
- Tests in `lpc-runtime/src/tree/node_tree.rs` (or sibling test
  module): root creation, `add_child` happy path, sibling-name
  collision returns `Err`, depth-first `remove_subtree` covers
  grandchildren, tombstone behaviour (slot stays `None`,
  `by_path` removed, no id reuse), `lookup_path` and
  `lookup_sibling` round-trips, `set_status` / `set_state` bump
  `change_frame`, parent's `children_ver` bumped on add/remove,
  `created_frame` is set on insert and not touched after.

Validation: `cargo test -p lpc-runtime`. Run the legacy
`ProjectRuntime` tests too — they stay green because nothing
about `ProjectRuntime` changed.

## P3 — `tree_deltas_since` on `NodeTree`

Translate the server tree's frame-versioning into `TreeDelta`
sequences.

- Add `lpc-runtime::tree::sync::tree_deltas_since` taking
  `&self, since: FrameId` and returning `Vec<TreeDelta>`.
- Algorithm:
  1. For every live entry whose `created_frame > since`: emit
     `TreeDelta::Created { … }` with all current fields.
     (Implies new entries don't *also* get `EntryChanged` /
     `ChildrenChanged` for `since=0` bulk.)
  2. For every entry whose `change_frame > since` AND
     `created_frame <= since`: emit `TreeDelta::EntryChanged`.
  3. For every entry whose `children_ver > since` AND
     `created_frame <= since`: emit
     `TreeDelta::ChildrenChanged`.
  4. Order: parent-before-child for `Created` (depth-first
     pre-order from root). `EntryChanged` and `ChildrenChanged`
     order is unconstrained.
- Add a `pub fn entry_state_view(&NodeEntry<N>) -> EntryStateView`
  helper (or a `From<&EntryState<N>> for EntryStateView` impl)
  so callers don't repeat the `match`.
- Tests in `lpc-runtime/src/tree/sync.rs` (or sibling test
  module): bulk export at `since=0`; no-op at
  `since=current_frame`; status change → single `EntryChanged`;
  add child → `Created` + parent's `ChildrenChanged`; remove
  subtree → no `Destroyed`, parent's `ChildrenChanged` only;
  consecutive frames each produce a tight diff.

Validation: `cargo test -p lpc-runtime`.

## P4 — `ClientNodeTree` mirror in `lp-engine-client`

The downstream consumer. Shows the wire round-trip works.

- Add files under `lp-engine-client::tree::*` per Design §file-structure.
- `ClientTreeEntry` per Design.
- `ClientNodeTree`:
  ```rust
  pub struct ClientNodeTree {
      pub nodes: BTreeMap<NodeId, ClientTreeEntry>,
      pub by_path: BTreeMap<NodePath, NodeId>,
      pub last_synced_frame: FrameId,
  }
  ```
  with `new()`, `get`, `lookup_path`, `last_synced_frame`.
- `apply_tree_delta(&mut self, delta: &TreeDelta, frame: FrameId)`:
  - `Created`: insert `ClientTreeEntry`, register `by_path`,
    update `last_synced_frame = max(self.last_synced_frame,
    frame)`. Add to parent's `children` if parent already exists
    (else: just sit there until parent's `Created` arrives).
    The server's pre-order `Created` emission means parents
    arrive first in practice; the post-condition tolerates
    out-of-order anyway.
  - `EntryChanged`: update entry's `status`, `state`,
    `change_frame`. Returns `Err(MissingNode(id))` if entry
    doesn't exist.
  - `ChildrenChanged`: replace `children` with the new list,
    update `children_ver`. **Diff** the old list against the
    new list: any id missing in the new list gets evicted
    (and its descendants, recursively). This is the inferred-
    removal step.
- Tests in `lp-engine-client/src/tree/apply.rs` (or sibling
  test module):
  - Bulk `apply_tree_delta` from `tree_deltas_since(0)` over a
    server tree with N nodes; assert client mirror matches.
  - Add → status change → child add → child remove sequence on
    the server; collect deltas at each frame; apply on client;
    assert mirror matches at every step.
  - Inferred removal: server removes a leaf; only `parent's
    ChildrenChanged` is sent; client correctly drops the leaf.
  - Inferred removal recursion: server removes a subtree;
    client drops every entry transitively.

Validation: `cargo test -p lp-engine-client`.

## Cross-cutting

- After P4: full workspace `just check`. None of the legacy
  paths should have changed; if they did we want to know.
- Documentation touch: `design/01-tree.md` and `design/07-sync.md`
  already reflect the M3 shape (this plan brought them up to date
  before phasing). No further design edits needed in M3.
- The next plan in this roadmap is the `Node` trait introduction,
  which un-comments the `EntryState::Alive(Box<dyn Node>)`
  substitution and starts pulling `NodeConfig` onto `NodeEntry`.
