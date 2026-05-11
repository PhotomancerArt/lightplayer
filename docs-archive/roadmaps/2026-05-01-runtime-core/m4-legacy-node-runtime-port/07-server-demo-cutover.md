# Phase 7: Wire lpa-server / just demo To CoreProjectRuntime

## Scope of Phase

Switch the MVP server/demo runtime path from `LegacyProjectRuntime` to
`CoreProjectRuntime` once the core runtime can load and tick the MVP project.

In scope:

- Update `lp-app/lpa-server/src/project.rs`.
- Update `lp-app/lpa-server/src/project_manager.rs`.
- Update server tick wiring as needed.
- Preserve enough compatibility wire responses/snapshots for clients to run.
- Keep old runtime code in the tree.
- Add or update server tests around loading/ticking projects.

Out of scope:

- Deleting `LegacyProjectRuntime`.
- Proper buffer/render-product sync refs; M4.1 owns that.
- Large UI/client refactors unless required for demo to run.
- New runtime feature flags that hide embedded shader compilation.

## Code Organization Reminders

- Keep server runtime ownership simple.
- Avoid long-lived dual-runtime switches unless needed temporarily for debugging.
- If a compatibility shortcut is needed, record it in `future.md`.
- Keep tests focused on load/tick/request behavior.

## Sub-agent Reminders

- Do not commit.
- This is a supervised phase: stay conservative and report blockers quickly.
- Do not suppress warnings or weaken tests.
- Do not add a broad compatibility layer unless the phase file explicitly calls
  for it.
- If app/server wiring exposes a design problem in `CoreProjectRuntime`, stop and
  report.
- Report changed files, validation results, and deviations.

## Implementation Details

Read first:

- `lp-app/lpa-server/src/project.rs`.
- `lp-app/lpa-server/src/project_manager.rs`.
- `lp-app/lpa-server/src/server.rs`.
- `lp-app/lpa-server/src/handlers.rs`.
- `lp-core/lpc-engine/src/project_runtime/`.
- `lp-core/lpc-engine/src/legacy_project/project_runtime/` for reference.
- `lp-cli/src/server/create_server.rs`.

Behavior:

- `lpa-server::Project` should own the new runtime for the MVP path.
- Project load should construct `CoreProjectRuntime`.
- Server tick should call the new runtime tick.
- Existing client requests should keep functioning through compatibility
  projection/snapshots where possible.

If a small temporary fallback to old runtime is needed to keep non-MVP behavior
alive, record it in `future.md` and make it explicit. Do not create an
unbounded dual runtime.

Tests:

- Server/project load constructs the new runtime.
- Server tick advances the new runtime.
- Existing simple project request tests still pass or are updated to the new
  compatibility projection.

## Validate

Run:

```bash
cargo test -p lpa-server
cargo test -p lpc-engine project_runtime
```
