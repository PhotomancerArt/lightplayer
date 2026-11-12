# Phase 3: Debug UI And Integration

## Scope of phase

In scope:

- Verify that the debug UI exposes editors for authored def leaves that now carry writable policy by default.
- Ensure end-to-end mutation behavior works against project-read responses and client pending state.
- Add any narrow client-side fixes needed to support the generalized authored-def mutation path cleanly.

Out of scope:

- New editor widgets beyond the current supported value-leaf editors.
- Runtime state editing.
- Container editing UX.

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep related functionality grouped together.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files and symbols:

- `lp-cli/src/debug_ui/slot_render.rs`
- `lp-cli/src/debug_ui/ui.rs`
- `lp-core/lpc-view/src/slot/mirror.rs`
- `lp-core/lpc-view/src/project/apply_project_read.rs`
- any existing slot-edit or debug-UI tests in `lp-cli`

Expected changes:

- Confirm no UI special-casing is needed once authored policies and generic engine mutation are in place.
- If needed, add a narrow integration test or fixture proving that authored writable leaves render editors while read-only/runtime leaves do not.
- Preserve the recent paged-shape-sync fix in `lp-cli/src/debug_ui/ui.rs` while validating the new mutation behavior against device-like shape sync.

Tests to add or update:

- `lp-cli` or `lpc-view` coverage showing pending mutation tracking still works with non-clock authored def targets.
- If practical, a debug-UI-oriented test around a representative authored def leaf.

Constraints and edge cases:

- Avoid UI-only hacks that bypass slot policy.
- Keep editor visibility policy-driven so future opt-outs behave naturally.

## Validate

```bash
cargo test -p lp-cli
cargo test -p lpc-view
```

