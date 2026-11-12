# Scope of Work

M3.1 prepares the state/config/wire projection layer for the M4 legacy runtime
port. The goal is to make the current legacy sync path trustworthy enough for
M4 parity work while drawing a clear boundary between runtime-owned products and
wire-visible state snapshots.

M3.1 should answer:

- how core-engine node state should be projected to clients during the M4/M5
  transition;
- which current legacy state fields are compatibility snapshots rather than
  authoritative runtime storage;
- whether `ProjectView` can correctly apply config and partial state details;
- whether `SerializableProjectResponse` is safe enough to evolve;
- what M3.2 must provide for texture/color/raw buffer identity.

In scope:

- Review and harden legacy `GetChanges` serialization/deserialization.
- Fix client-view application of `NodeDetail.config` and detail updates if the
  existing behavior is incomplete.
- Replace or un-ignore fragile serialization tests around
  `SerializableProjectResponse`.
- Document the projection boundary for texture data, fixture lamp colors, output
  channel data, shader state, and mapping cells.
- Define small names/contracts for "wire snapshot" vs "runtime product/buffer"
  without building the full buffer store.
- Add tests that protect partial-state merge behavior and config update behavior.

Out of scope:

- Implementing the full texture/buffer product store. That belongs in M3.2.
- Porting legacy shader/fixture/output runtime behavior to core `Engine`.
- Replacing `LegacyProjectRuntime`.
- Redesigning transport around binary chunks, compression, scaling, or throttled
  texture updates.
- Removing the legacy `GetChanges` protocol in this milestone.

# Current State

The current legacy client sync path is `ProjectResponse::GetChanges`, not the
newer structural `WireTreeDelta` stream.

`GetChanges` carries:

- `node_handles`: full current node set for pruning;
- `node_changes`: created/config/state/status/remove events;
- `node_details`: optional full detail for requested handles;
- `theoretical_fps`.

`NodeDetail` contains:

- `path`;
- `config: Box<dyn NodeConfig>`;
- `state: NodeState`.

Because `NodeDetail.config` is a trait object, `ProjectResponse` itself is not
serde-friendly. The wire path uses `SerializableProjectResponse` and
`SerializableNodeDetail` to downcast configs to concrete legacy types.

Current state payloads:

- `TextureState`
  - `texture_data: Versioned<Vec<u8>>`, base64 on the wire;
  - `width`, `height`, `format`.
- `FixtureState`
  - `lamp_colors: Versioned<Vec<u8>>`, base64 on the wire;
  - `mapping_cells`;
  - `texture_handle`, `output_handle`.
- `OutputState`
  - `channel_data: Versioned<Vec<u8>>`, base64 on the wire.
- `ShaderState`
  - shader/runtime diagnostic state.

The partial state serialization macro omits unchanged fields based on
`since_frame`, with all fields included on initial sync. Deserialization creates
state structs with default values, so client-side merge logic must distinguish
"field absent" from "field present with default-ish value" using per-type
merge conventions.

Known weak spots:

- `ProjectView::apply_changes` currently creates placeholder configs and does
  not actually copy `detail.config` into `NodeEntryView`.
- `ConfigUpdated` only updates `config_ver`; it relies on detail application to
  refresh config, but detail application also uses placeholders.
- `SerializableProjectResponse` has an ignored round-trip test with a TODO about
  custom serialization not matching deserialization.
- The state serialization macro has "temporary" language around direct
  `Serialize` implementations and is tightly coupled to legacy state structs.
- `WireTreeDelta` is a separate structural stream and does not carry large node
  payloads.
- Heavy fields are currently base64 JSON payloads in node state; this works for
  legacy compatibility but should not become core runtime storage.

Current core-engine state/domain boundary:

- `Production` carries `Versioned<RuntimeProduct>`.
- `RuntimeProduct::Render(RenderProductId)` is a handle into
  `RenderProductStore`.
- `RenderProductStore` can sample products, but it is test-oriented and does not
  define texture pixel storage, blob identity, wire snapshots, or diffs.
- `RuntimePropAccess` is explicitly documented as a legacy/data-only
  `LpsValueF32` bridge, not the runtime product envelope.

# Questions

## Q1: What should M3.1 call the boundary it is defining?

Context: We need vocabulary for values emitted to clients without implying that
wire state is the authoritative runtime product. Candidate names include
`StateProjection`, `WireProjection`, `NodeStateProjection`,
`LegacyStateProjection`, or `StateSnapshot`.

Answer: Use `SyncProjection` for the conceptual boundary and
`LegacySyncProjection` when referring specifically to the M4 compatibility path
that emits legacy `GetChanges`-style payloads.

Projection means a derived view of internal runtime/project state for a specific
consumer. Given a client frame id and watch/detail interests, project the current
state into a client-usable form that lets that client come up to speed. The
projected form may contain snapshots, deltas, versions, status, and compatibility
payloads, but it is not the authoritative runtime storage model.

## Q2: Should M3.1 change the wire format or only harden the existing one?

Context: The current wire path can serialize partial state and config through
`SerializableProjectResponse`, but it has fragile tests and client merge gaps.
Changing the external payload shape now could collide with M4 and client work.

Answer: Harden the existing wire shape. Do not introduce a new transport payload
format in M3.1. Add docs/types/tests that make the future projection boundary
explicit, then let M3.2 introduce store-backed product identity if needed.

## Q3: Should `ProjectView` store concrete config details from `NodeDetail`?

Context: `NodeDetail` has concrete configs on the server side, and
`SerializableNodeDetail` carries concrete config variants on the wire, but
`ProjectView::apply_changes` currently replaces configs with placeholders.

Answer: Yes. M3.1 should make `ProjectView` apply config details faithfully. If
cloning `Box<dyn NodeConfig>` is awkward, introduce a view-friendly enum or
helper rather than continuing placeholder configs.

## Q4: Should M3.1 un-ignore the `SerializableProjectResponse` round-trip test?

Context: An ignored test currently says deserialization does not match custom
serialization. M4 will likely evolve state payloads and needs confidence that
wire serialization is not silently one-way.

Answer: Yes, either fix and un-ignore the round-trip test or replace it with
focused tests that prove the supported wire direction and client apply path.
Prefer a real round-trip for `SerializableProjectResponse` if tractable.

## Q5: Should texture/lamp/output byte fields stay in legacy `NodeState` for M4?

Context: They are currently base64 `Versioned<Vec<u8>>` fields. Long term, these
should likely become store-backed buffers with refs/snapshots/diffs, but M4
needs behavior parity more than transport optimization.

Answer: Keep these fields as compatibility state snapshots for M4, but document
that they are not the authoritative runtime storage model. M3.2 should define
store-backed IDs and snapshot/diff vocabulary before M4 relies on them
internally.

## Q6: Does M3.1 need to bridge `WireTreeDelta` and legacy `GetChanges`?

Context: `WireTreeDelta` carries structural tree state, while `GetChanges`
carries legacy node config/state payloads. M4 may have a core `Engine` tree but
legacy-like UI/state consumers.

Answer: Do not merge the protocols in M3.1. Instead, document the two
projections and add tests around the legacy projection path. M3.3 can decide how
the core engine adapter emits or maps these projections during parity testing.

## Q7: What should M3.1 hand off to M3.2?

Context: The user explicitly called out texture store/wire representation,
color-list/raw-buffer domains, and not sending textures through normal state
streams forever.

Answer: M3.1 should produce a short inventory of heavy state fields and required
metadata for M3.2: texture bytes plus width/height/format, fixture lamp RGB
bytes, output channel bytes, mapping cells if needed, frame/version, and
intended snapshot/ref semantics.

# Notes

- M3.1 is a readiness milestone. It should be small enough to finish before
  M4, but strong enough that M4 can rely on client-visible state/config sync for
  parity tests.
- M3.1 should not preempt M3.2 by designing compression, throttling, scaling,
  binary transfer, or product eviction policies.
