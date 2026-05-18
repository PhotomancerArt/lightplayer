# Phase 5: Cleanup And Validation

## Scope Of Phase

Finalize the metadata-driven mockup codec generation plan execution.

In scope:

- Remove temporary debugging artifacts.
- Ensure no mockup Serde references returned.
- Update summary documentation.
- Run focused validation.
- Record remaining rough edges before production adoption.

Out of scope:

- Production adoption.
- Removing Serde from crates other than `lpc-slot-mockup`.
- Full CI validation unless explicitly requested.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `docs/roadmaps/2026-05-13-slot-native-streaming-serialization/m2-codegen-mockup/05-metadata-driven-codegen/summary.md`
- `docs/roadmaps/2026-05-13-slot-native-streaming-serialization/m2-codegen-mockup/summary.md`
- `lp-core/lpc-slot-codegen/src/lib.rs`
- `lp-core/lpc-slot-mockup/src/tests/generated_shape_codec.rs`

Expected changes:

- Add `summary.md` for this plan directory.
- Optionally update the parent M2 summary with the metadata-driven generation
  result.
- Search for debug output, stale TODOs, and direct mockup Serde references.
- Confirm generated root adapters are rendered from metadata.

Searches:

```bash
rg -n "dbg!|eprintln!|TODO|TEMP|HACK" lp-core/lpc-slot-codegen/src lp-core/lpc-slot-mockup/src lp-core/lpc-model/src/slot_codec
rg -n "serde|Serialize|Deserialize|serde_json" lp-core/lpc-slot-mockup
```

The Serde search should return no direct mockup references.

## Validate

```bash
cargo fmt
cargo test -p lpc-slot-codegen
cargo test -p lpc-model slot_codec
cargo test -p lpc-slot-mockup generated_shape_codec
cargo test -p lpc-slot-mockup
cargo check -p lpc-model --no-default-features
cargo check -p lpc-wire --no-default-features
cargo check -p lpc-slot-mockup
```
