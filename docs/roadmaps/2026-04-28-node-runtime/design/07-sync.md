# 07 — Client / server sync

The client is a thin mirror of the engine's tree. The client owns
no `Box<dyn Node>`s, runs no tick logic, holds no resources. It
holds **`NodeView`** snapshots: per-entry blobs of address +
status + the produced fields the wire ships. Everything is
populated from server deltas keyed on `FrameId`.

This is the most novel piece of the spine. Most prior art assumes
ui-and-engine in one process. We split them.

## Server side

`ProjectRuntime<D>` already (M2) carries:

```rust
pub struct ProjectRuntime<D: ProjectDomain> {
    pub frame_id: FrameId,
    // ... node tree, fs, output, etc. ...
}
```

Per-entry frame versions ([01](01-tree.md)):

- `status_ver` — when `status` changes.
- `config_ver` — when `NodeConfig` changes.
- `state_ver` — when any `Prop<T>` in `*Props` ticks forward.

The sync API:

```rust
impl<D: ProjectDomain> ProjectRuntime<D> {
    pub fn tick(&mut self, /* ... */) -> Result<(), Error>;

    /// Pull the per-entry diff vs `since_frame`.
    /// `detail_specifier` selects which entries to include in
    /// the response (None / All / ByHandles).
    pub fn get_changes(
        &self,
        since_frame: FrameId,
        detail_specifier: &ApiNodeSpecifier,
        theoretical_fps: Option<f32>,
    ) -> Result<D::Response, Error>;
}
```

`D::Response` is the domain-specific response payload (see
[08](08-domain.md)). For `LegacyDomain`, it's
`lpl_model::ProjectResponse` (already shipped). For a future
`VisualDomain`, it's a different shape that includes visual node
deltas.

The `get_changes` body is generic across domains:

1. Walk every entry whose `max(status_ver, config_ver, state_ver)
   > since_frame`.
2. For each, decide what to ship:
   - **`status_ver` advanced**: ship `NodeStatus` + new `status_ver`.
   - **`config_ver` advanced**: ship the new `NodeConfig` (or a
     diff thereof — TBD).
   - **`state_ver` advanced**: walk the entry's `Node::props()`
     via `iter_changed_since(since_frame)`, ship per-prop deltas.
3. Pack into `D::Response` and return.

Frame IDs increase monotonically; clients always know what they
last saw.

## Client side

The client maintains a tree mirror:

```rust
pub struct ClientView<R: ClientResponse> {
    nodes: HashMap<NodeId, NodeView>,
    by_path: HashMap<NodePath, NodeId>,
    last_synced_frame: FrameId,
}

pub struct NodeView {
    pub id: NodeId,
    pub path: NodePath,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub child_kinds: Vec<ChildKind>,

    pub state: EntryStateView,           // Pending / Alive / Failed
    pub status: NodeStatus,
    pub status_ver: FrameId,
    pub config_ver: FrameId,
    pub state_ver: FrameId,

    pub config: NodeConfig,              // mirror of authored data
    pub props: BTreeMap<PropPath, (LpsValue, FrameId)>,  // produced fields snapshot
}

pub enum EntryStateView {
    Pending,
    Alive,
    Failed(ErrorReason),
}
```

Differences from server-side `NodeEntry`:

- **No `Box<dyn Node>`.** The client doesn't run nodes.
- **No `EntryState::Alive(Box<dyn Node>)` payload** — just the
  `Alive` discriminant.
- **No resolver cache.** The client reads `config` (authored data,
  shipped via `config_ver` deltas) and computes its own resolution
  if it needs to display "current value." Or, more likely, the
  server ships current-resolved values for select slots in the
  delta and the client just displays them.
- **No `ArtifactRef`.** The client may keep its own
  `ArtifactManager` mirror (loading the same TOML files locally
  for editor previews) — that's a client-side decision, not
  spine-mandated.

## Delta protocol

```rust
pub struct NodeChange {
    pub id: NodeId,
    pub path: NodePath,
    pub diff: NodeDiff,
}

pub enum NodeDiff {
    Created  { kind: D::ArtifactKindTag, parent: Option<NodeId>,
               child_kind: ChildKind, config: NodeConfig },
    StatusChanged { status: NodeStatus, status_ver: FrameId },
    ConfigChanged { config: NodeConfig, config_ver: FrameId },
    PropsChanged  { entries: Vec<(PropPath, LpsValue)>, state_ver: FrameId },
    StateChanged  { state: EntryStateView },
    Destroyed,
}
```

(The legacy `lpl_model::NodeChange` has `Created` / `StateUpdated`
/ `StatusChanged` / `Destroyed`. M5 keeps the legacy variant set
for `LegacyDomain`; the generic shape above is the framing the
domains share.)

### What the wire ships

- **Bulk on first connect or on `since_frame = 0`:** every node's
  full `NodeView`. Includes config, status, props snapshot.
- **Per-frame deltas:** only entries with advanced `*_ver`. Tight.
- **`config_ver` deltas** are the heaviest payload. M5 ships the
  full new `NodeConfig`; future optimisation: ship the per-entry
  override-map diff. Defer until profiling justifies it.
- **`state_ver` deltas** are the most frequent. Per-prop deltas
  via `props().iter_changed_since(since)`. The producer side
  enforces a "stable" prop schema — the wire delta carries only
  changed fields, indexed by `PropPath`.

### Detail-specifier policy

Existing `ApiNodeSpecifier`:

```rust
pub enum ApiNodeSpecifier {
    None,                    // metadata only; no per-prop deltas
    All,                     // every node's full deltas
    ByHandles(Vec<NodeId>),  // selected nodes' full deltas
}
```

Stays. Editor uses `None` for tree-overview views; `ByHandles` for
"the user has node X open and wants live values." Cuts wire cost
when the editor is watching just a few nodes out of many.

## Backpressure / dropped frames

The client doesn't acknowledge per-frame; it just records
`last_synced_frame` and pulls again. If a frame batch is dropped
(server tick advanced past the client's poll period), the client
asks for the new `since_frame` next time and the server pulls
deltas vs the older frame. **Diffing is `since_frame`-keyed**, so
arbitrarily many frames between polls is fine — the server walks
once with the older `since`.

The wire layer (HTTP / WebSocket / serial) is `lp-server`'s
concern. The spine just exposes `get_changes(since_frame, ...)`.

## Hot reload affecting sync

When fs-watch reloads a parent's TOML
([04](04-config.md)) or an artifact ([03](03-artifact.md)):

1. Affected entries get `config_ver` bumped.
2. Tick proceeds; node impls observe via `ctx.changed_since` or
   `ctx.artifact_changed_since`.
3. `Node::tick` may produce different outputs; `state_ver` bumps
   per affected `Prop<T>::set`.
4. Next `get_changes` ships:
   - `ConfigChanged` for the parent (or the cascade descendants).
   - `PropsChanged` for whichever produced fields changed.

The client never sees an "fs reload" event explicitly — it just
sees the consequences (config + state deltas).

## Init-time tree creation

On first `get_changes(since_frame: 0, ...)`, the server returns
`Created` for every entry (with full `NodeConfig`) plus full
`PropsChanged` for every `Alive` entry's snapshot. The client
seeds its mirror.

For `Pending` and `Failed` entries: ship `Created` + the
`EntryStateView`. The client knows there's no `props()` data
because the discriminant tells it so. (Editor falls back to the
artifact's defaults — which it can read on its own — for "what
this node *would* look like once alive.")

## Status reporting

Uniform NodeStatus + frame versioning is the load-bearing F-1 from
prior art. M5 keeps it.

```rust
pub enum NodeStatus {
    Created,                       // entry exists, never woken
    InitError(String),             // D::instantiate failed (= EntryState::Failed)
    Ok,                            // running normally
    Warn(String),                  // soft issue (e.g., binding type mismatch)
    Error(String),                 // tick failed; entry stays Alive but unhealthy
}
```

Already shipped in `lpc_model::project::api::NodeStatus`. M5 may
add discriminants if Pending needs surfacing distinctly from
Created. Lean: keep `Created` as the catch-all "not yet alive"
state and read `EntryState` for fine-grained detail; clients that
care can read both.

## Why the wire ships `LpsValue` and not typed `T`

The wire is structurally typed. The client doesn't have the impl's
typed `*Props` struct (and shouldn't — `lpl-runtime` is server-only,
the client compiles for browser / mobile / desktop without
shader-compile machinery). `LpsValue` is the lingua franca.

Lossy round-trip is acceptable:

- `Prop<TextureBuffer>` ships as `LpsValue::Texture(<metadata>)`
  on the wire — the editor doesn't need pixel data, it needs a
  thumbnail (which goes through a separate request channel).
- `Prop<ShaderProgram>` doesn't ship to the client; it's
  server-internal. The `PropAccess` derive flags it as
  `#[prop(state, server_only)]` (or just doesn't include it in
  `iter_changed_since`).

## What the client does *not* ship back

The client→server direction has just three operations in M5:

- `set_property(node_path, prop_path, value)` — edit an override.
- `get_changes(since_frame, detail_specifier, ...)` — the poll.
- `subscribe_to_node(node_path)` — declare interest (so server
  ships `ByHandles` deltas without the client needing to enumerate).

That's it. The client never sends node creates, destroys, or fs
edits — those go through the `lp-server`-level filesystem API
(out of spine scope; M2 wires `FsRequest` / `FsResponse` already).

## Open questions

- **Wire format.** JSON via `serde_json` (M2 default) or a tighter
  binary encoding (postcard / bincode)? M5 keeps JSON; future
  ESP32 / mobile profiling may force binary. Pin in M5
  implementation.
- **Per-prop ship granularity.** `outputs[0]` is a `LpsValue::Vec3`
  on a 60-pixel-wide texture node — that's a struct of 60×3 floats
  every frame. Ship full or ship sub-paths? Lean ship-full for
  M5; M6 cleanup adds per-LpsValue diff if needed.
- **`Created` payload size.** The wire ships full `NodeConfig` on
  create. For a freshly-loaded project with hundreds of nodes,
  that's a single big response. Tolerable; profile.
- **Channel of cascade updates.** When a `Live` cascade
  rematerialises, *every affected descendant* gets a
  `ConfigChanged` delta, not the cascade source. Is that a
  surprise the editor needs to be told ("config X changed because
  the cascade Y on the ancestor moved")? M5 ships descendant-only;
  the editor can re-derive the cascade source if it cares.
