# Phase 3: Hardware Validation

## Scope of phase

In scope:

- add a dedicated firmware-side incremental compile test/harness for ESP32
- exercise representative shaders under a fixed per-tick compile budget
- log per-tick compile timing and memory usage
- produce a practical validation path for the future warm-up use case

Out of scope:

- final product playlist/timeline feature
- non-test refactors unrelated to supporting the harness

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep test/harness code isolated under firmware test directories when possible.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

- Likely touch points:
  - [lp-fw/fw-esp32/src/tests](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-fw/fw-esp32/src/tests)
  - [lp-fw/fw-esp32/src/server_loop.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-fw/fw-esp32/src/server_loop.rs) only if shared timing/memory hooks are needed
  - [lp-core/lpc-engine/src/nodes/shader/shader_node.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-core/lpc-engine/src/nodes/shader/shader_node.rs) only if the harness needs compile stepping entry points
- Add an on-hardware test that:
  - compiles several representative example shaders incrementally
  - uses a fixed compile budget per tick
  - logs per tick:
    - compile stage / progress
    - elapsed compile slice time
    - free / used memory
  - reports completion totals
- Prefer representative shaders such as:
  - a tiny trivial shader
  - `examples/basic`
  - a texture-using example if practical without adding too much harness complexity
- If exact timing hooks need new instrumentation, keep them narrow and reusable.
- Add host/emu validation where useful, but the on-device path is the primary deliverable.

## Validate

Run:

```bash
cargo fmt --all
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

If the harness is runnable through a `just` recipe or existing firmware test flow, run that too and record the measured tick/memory behavior.
