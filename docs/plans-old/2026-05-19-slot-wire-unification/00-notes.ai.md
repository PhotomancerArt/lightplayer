# Slot Wire Unification Notes

## Scope

Recent SlotCodec/domain-serialization work left project-read node slot payloads
with two competing encodings behind one `WireSlotData` field. This plan covers
making the client/server slot sync path single-format again, removing Serde
bindings from `SlotData`, fixing the shape registry paging bug that exposed the
issue, and adding tests that prevent the debug UI from regressing.

Out of scope:

- Removing `SlotData` from the model/runtime.
- Removing Serde from every wire envelope.
- Reworking large resource payload streaming.
- Changing the on-device compiler path.

## User Context

- The debug UI reports:
  `slot apply error: invalid slot root data: root node.0.def shape 0x8acdff63 did not decode as SlotCodec ... or SlotData ...`.
- Nodes then show `no def config slot` and no outputs.
- Resources can appear in the UI, but payload contents are not reachable.
- The user called out that fallback decoding is a smell and wants one solid
  decode path rather than SlotCodec-or-SlotData guessing.

## Current State

### `WireSlotData` Has No Declared Internal Format

Relevant files:

- `lp-core/lpc-wire/src/slot/sync.rs`
- `lp-core/lpc-view/src/slot/mirror.rs`

`WireSlotRootSnapshot.data` is a `RawValue` wrapper named `WireSlotData`.
Nothing in the type says whether the raw JSON is:

- legacy `SlotData` JSON, with `kind`, `fields_revision`, per-leaf revisions,
  indexed record fields, and tagged map keys; or
- SlotCodec JSON, with shape-driven record field names, enum `kind`, concise map
  objects, omitted defaults, and no sync revisions.

`SlotMirrorView::read_wire_slot_root` currently tries:

1. `SlotShapeRegistry::read_slot_json_data(root.shape, root.data.get())`
2. fallback to `wire_slot_data_to_slot_data`

That fallback is the visible symptom of an unfinished migration.

### `SlotData` Still Has Serde Bindings

Relevant files:

- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-wire/src/slot/sync.rs`
- `lp-core/lpc-wire/src/slot/access_sync.rs`

`SlotData`, `SlotRecord`, `SlotMapDyn`, `SlotMapKey`, `SlotEnum`, and
`SlotOptionDyn` derive `serde::Serialize` and `serde::Deserialize`.

That keeps the legacy path alive:

- `wire_slot_data_from_slot_data` serializes `SlotData` with
  `serde_json::value::to_raw_value`.
- `wire_slot_data_to_slot_data` deserializes `WireSlotData` with
  `serde_json::from_str`.
- `WireSlotChange::Replace(SlotData)` makes incremental slot patches depend on
  `SlotData` Serde too.

If the goal is to remove Serde from slotted data for firmware code size, these
bindings should be removed as part of the cleanup, not merely bypassed in one
project-read path.

### There Are Two Project-Read Producers With Different Slot Encodings

Relevant files:

- `lp-core/lpc-engine/src/engine/project_read_nodes.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-core/lpc-shared/src/transport/server.rs`
- `lp-app/lpa-server/src/server.rs`
- `lp-app/lpa-server/src/handlers.rs`

Allocated project reads (`Engine::read_project`) build node slot roots through
`snapshot_slot_root` and `wire_slot_data_from_slot_data`, so `data` is legacy
`SlotData` JSON.

Streaming project reads (`Engine::write_project_read_json`) write root `data`
through `SlotShapeRegistry::write_slot_json_value`, so `data` is SlotCodec JSON.

`lpa-server::server` uses the streaming source for project requests. The default
`ServerTransport::send_project_read` writes JSON, then deserializes the envelope
back into `ProjectReadResponse`; because `WireSlotData` is raw JSON, this keeps
the SlotCodec payload intact. Other direct call sites and tests can still use
the allocated `Engine::read_project` path and get legacy `SlotData` payloads.

The same public type therefore carries different slot syntaxes depending on
which producer path created it.

### SlotCodec JSON Is Not A Sync Snapshot

Relevant files:

- `lp-core/lpc-model/src/slot_codec/dynamic_slot_writer.rs`
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`
- `lp-core/lpc-model/src/slot/slot_data.rs`
- `lp-core/lpc-view/src/slot/apply.rs`

SlotCodec JSON is well suited to authored/value payloads. It is not currently a
lossless client sync snapshot:

- It does not encode `Revision`s.
- The reader stamps values and containers with ambient `current_revision()`.
- The writer may omit empty/default fields and the reader recreates them.
- Record data is name-addressed in JSON, while `SlotData::Record` stores indexed
  fields interpreted through `SlotShape`.

The client mirror uses `SlotData` revisions for mutation conflict checks via
`data_version_at`. If a full sync root is decoded from SlotCodec JSON, those
versions are not the server's actual data versions.

### Shape Paging Skips Shapes

Relevant files:

- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-cli/src/debug_ui/ui.rs`

`SlotShapeRegistry::snapshot_page(after, limit)` filters entries with
`id > after`. When it reaches the limit, it returns `next = Some(id)` for the
first omitted id.

The debug UI then sends that `next` back as `after`, causing the next request to
filter `id > next`. The omitted id is skipped permanently. With
`limit: Some(1)`, the UI can sync roughly every other shape.

The failing shape `0x8acdff63` is the static id for
`lpc_model::nodes::project::project_def::ProjectDef`, computed by
`SlotShapeId::from_static_name(concat!(module_path!(), "::", "ProjectDef"))`.
Missing that shape makes the root `node.0.def` impossible to decode.

### Slot Apply Errors Block Later Results

Relevant files:

- `lp-core/lpc-view/src/project/apply_project_read.rs`
- `lp-cli/src/debug_ui/ui.rs`

Debug UI project reads request results in this order:

1. optional shape page
2. nodes
3. resources
4. runtime

`apply_project_read_response` returns immediately on a slot apply error in the
nodes result. Resource summaries/payloads in the same response are not applied.
That explains why resources may be listed from earlier responses while selected
payload contents never arrive after the slot failure starts.

### Existing Tests Cover The Happy Path Only

Relevant files:

- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `lp-cli/src/debug_ui/ui.rs`

The streaming project-read tests prove SlotCodec JSON can read through the full
server registry and can apply to a view when that full registry is present.
They do not prove:

- only one `WireSlotData` syntax exists;
- the allocated and streaming project-read paths produce the same slot data
  syntax;
- paged shape sync includes every shape before slots are requested;
- decoded sync data preserves server revisions;
- resources still apply if slot data is malformed.

I also fixed a test compile blocker while investigating:
`lp-cli/src/debug_ui/ui.rs` used the removed `SlotShapeRegistry::register_root`
API. The test now calls `register_shape`.

## Open Questions

### Q1. What should be the canonical project-read slot sync format?

Context: `SlotData` is already the client mirror's in-memory format and carries
sync revisions. SlotCodec JSON is concise and shape-owned, but currently loses
revision data.

Suggested answer: keep `SlotData` as the in-memory snapshot model, but stop
using Serde-derived `SlotData` JSON as the wire contract. Add a dedicated
slot-sync snapshot codec that writes/reads a lossless `SlotData` snapshot from
`SlotDataAccess` through `SlotShapeRegistry`, preserving revisions and container
structure. Use that same codec for root snapshots and patch replacement
payloads. Keep authored SlotCodec JSON/TOML for authored/value payloads.

### Q2. Should `WireSlotData` remain raw JSON?

Context: raw JSON lets firmware stream a project-read response without building
a full JSON value tree. The problem is not raw JSON itself; the problem is that
raw JSON has no single format.

Suggested answer: keep raw JSON for transport efficiency, but rename/wrap it as
a sync snapshot payload and expose only one constructor/writer and one reader.
Remove the SlotCodec-or-SlotData fallback.

### Q3. Should `SlotData` continue deriving Serde?

Context: the old `SlotData` JSON derive is the other serialization domain. As
long as the derives remain, it is easy for new wire code to accidentally revive
the legacy path, and firmware can still pay for generic Serde implementations.

Suggested answer: no. Once root snapshots and patch replacements use the
canonical sync codec, remove Serde derives and helper modules from `SlotData`
and its owned containers. Tests should prove the model still has `SlotData` as
an in-memory type but no Serde-based wire API.

### Q4. Should the typed local transport be redesigned now?

Context: the default local/desktop transport currently sends typed
`WireServerMessage` values. It can still carry raw JSON inside `WireSlotData`.
The ESP32 transport already streams a raw server-message frame.

Suggested answer: do not make transport raw frames the center of this fix. First
make both allocated and streaming project-read producers emit the same canonical
slot snapshot JSON. Then the typed local transport remains usable without
semantic drift.

### Q5. Should shape paging be fixed independently?

Context: the paging bug can hide any root shape from the client. It is
independent of SlotCodec vs SlotData and should not be left as a debug UI quirk.

Suggested answer: yes. Define `next` as the cursor to send back to `after`,
which means it should be the last included id when more entries remain. Add a
limit-1 paging test that reconstructs all registry ids without skips.

### Q6. Should `apply_project_read_response` fail the whole response on one bad
domain?

Context: hard failure is useful for tests, but bad slot data prevents resource
payloads and runtime status from updating in the debug UI.

Suggested answer: keep the strict API as-is for core tests during the canonical
format cleanup. Add debug-UI or view-level error aggregation only after the slot
wire format is single-path; otherwise it can hide the real protocol bug.

## Validation Notes

Commands run during investigation:

```bash
cargo test -p lp-cli paged_shape_sync_keeps_prior_pages_when_final_page_is_complete -- --nocapture
cargo test -p lpc-engine streaming_project_read_slot_payloads_deserialize_and_apply_to_view -- --nocapture
cargo test -p lpc-engine streaming_project_read_slot_payloads_read_through_slot_codec -- --nocapture
```

All three passed after the small `register_shape` test update.
