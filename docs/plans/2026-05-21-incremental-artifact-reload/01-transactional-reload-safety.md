# Phase 1: Transactional Reload Safety

## Scope Of Phase

Make the current full-project reload fallback safe. Reload failure must not
leave `Project.runtime` absent and must not cause `engine()`/`engine_mut()` to
panic on the next tick or project read.

Out of scope:

- True incremental reload.
- Node-local SVG error state on fresh load.
- UI changes.

## Code Organization Reminders

- Keep server wrapper safety in `lp-app/lpa-server/src/project.rs`.
- Keep filesystem version policy in `lp-app/lpa-server/src/server.rs`.
- Avoid broad refactors while stabilizing the panic path.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update `Project::reload` in `lp-app/lpa-server/src/project.rs`:

- Build `EngineServices`.
- Call `ProjectLoader::load_from_root`.
- Set graphics on the new runtime.
- Only after all of that succeeds, replace `self.runtime`.
- Remove `drop(self.runtime.take())` before successful load.
- Ensure `backtrace::clear_oom_context()` happens on both success and failure
  if practical.

Update `LpServer::tick` in `lp-app/lpa-server/src/server.rs`:

- When `project.reload()` returns an error, log the error with enough context.
- Advance `project.last_fs_version` past the processed changes even on reload
  failure. Otherwise a broken intermediate SVG save will retrigger every tick.
- Keep ticking the existing runtime.

Add/extend tests in `lp-app/lpa-server/tests/fs_version_tracking.rs`:

- Load a minimal valid project.
- Modify a file so reload fails.
- Tick server.
- Assert no panic.
- Assert project still has a runtime/revision can be read.
- Assert `last_fs_version` advanced past the invalid change.
- Assert a following tick does not repeatedly process the same invalid change.

## Validate

```bash
cargo test -p lpa-server --test fs_version_tracking
cargo check -p lpa-server
```
