# Phase 5: Cleanup And Validation

## Scope Of Phase

Remove temporary fallback code where it is no longer needed, tighten tests, and
run final validation.

Out of scope:

- New feature work beyond the reload/error isolation fix.
- UI implementation.

## Code Organization Reminders

- Remove stale comments that describe full reload as the normal file-change
  behavior.
- Keep temporary full reload fallback clearly scoped to `project.toml` or other
  unsupported structural edits.
- Keep tests at the bottom of Rust files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Cleanup checklist:

- Search for unconditional `project.reload()` on file changes and remove or
  constrain it.
- Search for `expect("project runtime is only absent while reloading")`; decide
  whether it remains valid after transactional reload or should become a
  recoverable server error.
- Verify invalid SVG errors are not logged only; they must be visible through
  node status.
- Verify old runtime is retained on hot reload prepare failure.
- Verify fresh bad node load does not abort project load.
- Verify no changes weaken shader compiler availability or embedded paths.

Final validation commands:

```bash
cargo fmt --check
cargo check -p lpc-engine
cargo test -p lpc-engine artifact_reload
cargo test -p lpc-engine project_loader
cargo test -p lpc-engine project_read
cargo test -p lpa-server --test fs_version_tracking
cargo check -p lpa-server
cargo check -p lp-cli
```

If the change touches shader reload/compile behavior, also run targeted shader
pipeline validation appropriate to the touched crates.
