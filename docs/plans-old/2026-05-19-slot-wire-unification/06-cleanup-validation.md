# Phase 6: Cleanup And Validation

## Scope Of Phase

Remove temporary compatibility and prove the final slot wire path is clean.

In scope:

- remove unused fallback helpers and imports;
- update names/docs that still imply SlotCodec is the project-read sync format;
- verify `SlotData` and owned slot data containers have no Serde derives,
  imports, or helper modules;
- verify no wire type contains `SlotData` solely to get Serde for slotted data;
- run final validation;
- fix warnings and formatting issues.

Out of scope:

- broad transport redesign;
- removing Serde from every protocol type;
- changing authored TOML serialization.

## Code Organization Reminders

- Keep file and symbol names explicit: use `snapshot`, `sync`, or `wire` for
  sync paths, and `slot_codec` for authored/value paths.
- Avoid generic names such as `data_json` when the format matters.
- Tests stay at the bottom of Rust files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Search targets:

```bash
rg -n "did not decode as SlotCodec|or SlotData|wire_slot_data_to_slot_data|wire_slot_data_from_slot_data|read_slot_json_data|write_slot_json_value|SlotCodec.*project-read|project-read.*SlotCodec" lp-core lp-app lp-cli docs
rg -n "serde::Serialize|serde::Deserialize|#\\[serde|SerializeSeq|Deserializer|Serializer" lp-core/lpc-model/src/slot/slot_data.rs
rg -n "WireSlotChange::Replace\\(SlotData|Replace\\(SlotData\\)|serde_json::.*SlotData|to_raw_value\\(data\\)|from_str\\(data\\.get\\(\\)\\)" lp-core/lpc-wire lp-core/lpc-view lp-core/lpc-engine
```

Expected cleanup:

- no client mirror fallback text remains;
- project-read sync docs mention slot sync snapshot codec, not authored
  SlotCodec;
- `SlotData` and its owned containers do not derive or implement Serde;
- incremental patch replacements use the same slot sync snapshot codec as root
  snapshots;
- old helper exports are either removed or renamed to their remaining actual
  purpose;
- any TODOs added during phases are resolved or moved into a follow-up note.

Final validation commands:

```bash
cargo fmt --check
cargo test -p lpc-model slot_sync_codec
cargo test -p lpc-wire source_slot_sync
cargo test -p lpc-view slot
cargo test -p lpc-engine project_read_stream
cargo test -p lp-cli debug_ui
cargo check -p lpa-server
cargo test -p lpa-server --no-run
rg -n "serde::Serialize|serde::Deserialize|#\\[serde|SerializeSeq|Deserializer|Serializer" lp-core/lpc-model/src/slot/slot_data.rs
rg -n "did not decode as SlotCodec|or SlotData|wire_slot_data_to_slot_data|wire_slot_data_from_slot_data|WireSlotChange::Replace\\(SlotData|Replace\\(SlotData\\)|serde_json::.*SlotData|to_raw_value\\(data\\)|from_str\\(data\\.get\\(\\)\\)" lp-core lp-app lp-cli
```

The final `rg` commands should produce no matches. If any Serde usage remains
under `lpc-model/src/slot`, it must be intentionally outside owned slotted data
and called out in the implementation summary with a follow-up or rationale.

If the implementation touches shader pipeline behavior beyond project-read
debug sync, also run the shader-pipeline commands from `AGENTS.md`.
