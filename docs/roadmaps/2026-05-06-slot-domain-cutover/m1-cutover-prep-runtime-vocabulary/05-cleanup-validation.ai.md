# Phase 5: Cleanup And Validation

## Scope Of Phase

In scope:

- Remove temporary leftovers from M1.
- Verify legacy and slot vocabulary is searchable and intentional.
- Run final focused validation.
- Update plan notes with any deviations.

Out of scope:

- Starting M2 source def slot roots.
- Removing legacy detail sync.
- Client mutation.

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

Cleanup checks:

- `rg "WireNodeSpecifier|ProjectResponse|SerializableProjectResponse|NodeChange|NodeDetail|NodeState" lp-core lp-cli lp-app`
  - Remaining hits should either be in explanatory docs or intentionally legacy names inside comments that should be updated.
- `rg "ValuePath" lp-core/lpc-engine/src/{prop,resolver,binding,bus,nodes,engine,node}`
  - Runtime demand path should not use `ValuePath` for produced/consumed identity.
  - Legacy authored resolver path may still use `ValuePath`.
- Confirm `WireProjectRequest::GetChanges` has both `legacy_detail_specifier` and `slot_watch_specifier`.
- Confirm `SlotMeta::writable` defaults to false.

## Validate

```bash
cargo fmt -p lpc-model -p lpc-wire -p lpc-view -p lpc-engine -p lpa-client -p lpa-server -p lp-cli
cargo test -p lpc-model
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-engine
cargo test -p lp-cli
cargo check -p lpa-client
cargo check -p lpa-server
git diff --check
```

