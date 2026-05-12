# Phase 7: Debug UI Staging And Final Validation

## Scope Of Phase

Keep debug UI reads low-bandwidth and finish plan cleanup/validation.

In scope:

- Stage debug UI project reads so shapes, slot roots, resources, and probes are
  not all requested unnecessarily.
- Remove temporary/debug-only code that corrupts JSON or adds noise.
- Ensure OOM diagnostics remain useful and non-allocating.
- Run final validation.

Out of scope:

- Rebuilding the real UI.
- Adding persistent subscriptions.
- Adding product/resource probes beyond existing behavior.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Do not leave temporary instrumentation in hot paths unless it is explicitly
  guarded and documented.
- Put helpers lower in files when that improves readability.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-cli/src/debug_ui/ui.rs`
- `lp-cli/src/debug_ui/*`
- `lp-fw/fw-esp32/src/main.rs`
- `lp-fw/fw-esp32/src/serial/io_task.rs`
- `docs/plans/2026-05-12-project-read-end-to-end-streaming/*`

Expected changes:

- Debug UI should request shapes only when local registry is empty or stale.
- Debug UI should request detailed slot roots only when needed.
- Steady-state polls should prefer node/resource summaries and targeted
  resource payloads.
- Remove accidental WIP helpers not used by final design.
- Remove heap breadcrumbs that write through raw `esp_println` during an active
  `M!` frame.
- Keep non-allocating OOM printout in `fw-esp32/src/main.rs`.

Final validation:

- Run focused unit tests.
- Run ESP32 check.
- If hardware is available, run `just demo-esp32c6-host` or the current project
  demo command and confirm debug UI project read no longer OOMs.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-wire
cargo test -p lpc-view
cargo test -p lpc-engine project_read
cargo test -p lpa-server
cargo check -p lp-cli
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

