# Phase 3: Output Provider Cutover

## Scope Of Phase

In scope:

- Change `OutputProvider::open` to accept endpoint specs.
- Update `EngineServices` sink bindings.
- Update memory, emu, ESP32, server wrapper, and test wrapper providers.
- Route WS281x opens through `HardwareSystem::open_ws281x_by_spec`.

Out of scope:

- Keeping numeric pin provider methods.
- Changing output write/close semantics.

## Code Organization Reminders

- Keep provider APIs capability-facing, not GPIO-facing.
- Avoid compatibility overloads unless they are private test helpers.
- Tests at file bottom.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-shared/src/output/provider.rs`
- `lp-core/lpc-shared/src/output/memory.rs`
- `lp-fw/fw-emu/src/output.rs`
- `lp-fw/fw-esp32/src/output/provider.rs`
- `lp-app/lpa-server/src/project.rs`
- `lp-core/lpc-engine/src/engine/engine_services.rs`
- `lp-core/lpc-engine/src/engine/output_flush_tests.rs`

Expected changes:

- `OutputSinkBinding` stores `endpoint: HardwareEndpointSpec`.
- `EngineServices::register_output_sink` reads `config.endpoint()`.
- `ensure_channel_open` passes the endpoint to the provider.
- Memory/emu/ESP32 providers use `open_ws281x_by_spec`.
- Test helper APIs should switch from `is_pin_open` where practical to
  endpoint/resource assertions. If keeping `is_pin_open` for registry
  inspection, do not use it as the public authoring path.

## Validate

```bash
cargo test -p lpc-shared output
cargo test -p lpc-engine engine_services
cargo test -p lpc-engine output_flush
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

