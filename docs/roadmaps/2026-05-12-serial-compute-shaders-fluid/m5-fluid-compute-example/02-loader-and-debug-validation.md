# Phase 2: Loader And Debug Validation

## Scope Of Phase

Add focused validation that `examples/fluid` loads and exposes useful runtime state.

In scope:

- Add or extend a project-loader test to load `examples/fluid`.
- Resolve the fluid output and render/sample enough data to prove nonzero output.
- Exercise project read/debug path enough to confirm compute/fluid state roots appear.

Out of scope:

- Pixel-perfect output assertions.
- UI snapshot tests.
- ESP32 execution.

## Code Organization Reminders

- Prefer test helpers near existing project loader tests if this is primarily an engine integration test.
- Keep test data in `examples/fluid`; do not duplicate large TOML strings unless necessary.
- Tests stay at the bottom of files.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-engine/src/engine/project_read.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`
- `examples/fluid/*`

Expected assertions:

- `ProjectLoader::load_from_root` can load `examples/fluid`.
- The compute node can produce `emitters`.
- The fluid node can produce `output`.
- Rendering the fluid visual product yields at least one nonzero channel.
- A debug project read includes node runtime state roots for compute/fluid.

Use existing test patterns before adding new harness machinery.

## Validate

```bash
cargo test -p lpc-engine fluid -- --nocapture
cargo test -p lpc-engine project_loader -- --nocapture
cargo test -p lpc-engine project_read -- --nocapture
```
