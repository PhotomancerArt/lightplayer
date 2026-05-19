# Phase 4: Client View Strict Decode

## Scope Of Phase

Remove the SlotCodec-or-SlotData fallback from the client slot mirror and make
root decode strict.

In scope:

- update `SlotMirrorView::read_wire_slot_root`;
- update patch application to decode raw replacement payloads through the target
  slot shape;
- update slot mirror tests;
- improve error text;
- prove revisions survive full sync into mutation preparation.

Out of scope:

- changing mutation request/response shapes;
- adding debug UI error aggregation;
- changing authored SlotCodec behavior.

## Code Organization Reminders

- Keep public mirror methods at the top of `mirror.rs`.
- Keep private decode helpers below mutation helpers.
- Put tests at the bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-view/src/slot/mirror.rs`
- `lp-core/lpc-view/src/slot/apply.rs`
- `lp-core/lpc-wire/src/slot/sync.rs`

Current behavior:

```text
try registry.read_slot_json_data(...)
else try wire_slot_data_to_slot_data(...)
```

Expected behavior:

```text
read wire slot sync snapshot payload once
```

Suggested changes:

- Replace `read_wire_slot_root` with a call to the new wire/model sync snapshot
  reader.
- Replace `apply_patch` handling of `WireSlotChange::Replace(SlotData)` with:
  - resolve the patch path to the concrete target shape;
  - decode the raw replacement payload through the sync snapshot reader for that
    target shape;
  - assign the decoded `SlotData`.
- Error text should name the single expected format, e.g.
  `invalid slot sync snapshot for root ...`.
- Remove imports and exports for old fallback helpers if no longer needed.
- Add tests:
  - full root snapshot applies with expected data;
- malformed SlotCodec-style JSON is rejected as invalid sync snapshot;
- malformed patch replacement JSON is rejected as invalid sync snapshot;
- missing shape reports `missing slot shape`;
- `prepare_set_value` after full sync uses the server-provided value revision,
  not ambient/default revision.

Keep in mind:

- `SlotMirrorView` should still store `SlotData`.
- No client-side code should deserialize `SlotData` with Serde.

## Validate

```bash
cargo test -p lpc-view slot
cargo test -p lpc-engine streaming_project_read_slot_payloads_deserialize_and_apply_to_view -- --nocapture
rg -n "wire_slot_data_to_slot_data|did not decode as SlotCodec|or SlotData|serde_json::from_str.*SlotData" lp-core/lpc-view lp-core/lpc-wire
```

The `rg` command should produce no matches.
