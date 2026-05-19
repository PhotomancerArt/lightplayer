# Phase 1: Shared Endpoint Specs

## Scope Of Phase

In scope:

- Add the endpoint spec value type.
- Add endpoint specs to `HardwareEndpoint`.
- Have WS281x drivers advertise specs.
- Add `HardwareSystem::open_ws281x_by_spec`.
- Keep existing address/id opens only for lower-level compatibility and tests.

Out of scope:

- Changing `OutputDef`.
- Changing `OutputProvider`.
- Button/radio node authoring.

## Code Organization Reminders

- Prefer one concept per file.
- Put tests at the bottom of Rust files.
- Keep parsing intentionally small: exactly `cap:driver:config`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/`
- `lp-core/lpc-shared/src/hardware/mod.rs`
- `lp-core/lpc-shared/src/hardware/hardware_endpoint.rs`
- `lp-core/lpc-shared/src/hardware/hardware_system.rs`
- `lp-core/lpc-shared/src/hardware/virtual_ws281x_driver.rs`
- `lp-fw/fw-esp32/src/output/rmt_ws281x_driver.rs`

Expected changes:

- Add `HardwareEndpointSpec` as a slot-capable string value. Prefer
  `lpc-model` for the owning type so `OutputDef` can use it without making
  `lpc-model` depend on `lpc-shared`.
- Re-export the type where useful.
- `HardwareEndpoint` should store `spec`.
- `HardwareEndpoint::new` callers must pass the spec.
- `VirtualWs281xDriver` should advertise specs such as `ws281x:rmt:D10` when a
  manifest resource has display label `D10`, and should also be able to support
  virtual/test configs already used by tests.
- `Esp32RmtWs281xDriver` should advertise `ws281x:rmt:D10`.
- `HardwareSystem::open_ws281x_by_spec` should exact-match endpoint specs.

Tests to add/update:

- Endpoint spec parse rejects malformed strings.
- Virtual WS281x endpoints expose expected specs.
- `HardwareSystem::open_ws281x_by_spec("ws281x:rmt:D10")` claims the right
  resources on the virtual board.
- Unknown spec returns a clear unknown endpoint error.

## Validate

```bash
cargo test -p lpc-model hardware_endpoint_spec
cargo test -p lpc-shared hardware
cargo check -p lpc-shared --no-default-features
```

