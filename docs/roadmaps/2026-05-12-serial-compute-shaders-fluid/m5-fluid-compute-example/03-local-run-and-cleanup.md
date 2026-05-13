# Phase 3: Local Run And Cleanup

## Scope Of Phase

Run the example through normal developer entry points, clean up rough edges, and document the result.

In scope:

- Run host/dev validation for `examples/fluid`.
- Run focused cargo checks/tests.
- Update the milestone summary.
- Add follow-up notes if runtime, debug UI, or profiling gaps show up.

Out of scope:

- Required ESP32 profiling.
- Debug UI redesign.
- Solver tuning.

## Code Organization Reminders

- Do not leave temporary logs or generated output in source directories.
- Keep summary concise and factual.
- If `target/` evidence is created, do not commit it.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Useful commands:

```bash
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-engine
cargo run -p lp-cli -- dev examples/fluid
```

If `lp-cli dev examples/fluid` needs a display/server interaction that cannot be completed in the current environment, record what was attempted and why it was stopped.

Write:

```text
docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m5-fluid-compute-example/summary.md
```

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model
cargo test -p lpc-engine
cargo check -p lpc-engine
```
