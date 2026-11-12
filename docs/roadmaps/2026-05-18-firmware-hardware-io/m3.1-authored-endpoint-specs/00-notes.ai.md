# M3.1 Authored Endpoint Specs Notes

## Scope Of Work

Replace legacy numeric output pin authoring with a required terse hardware
endpoint spec string:

```text
cap:driver:config
```

Initial concrete output shape:

```toml
kind = "Output"
endpoint = "ws281x:rmt:D10"
```

This plan should:

- introduce a simple parsed endpoint spec type;
- have drivers advertise endpoint specs they can open;
- have `HardwareSystem` open hardware by exact endpoint spec, not decoded
  fallback pin logic;
- change `OutputDef` from `pin` to required `endpoint`;
- update output flushing and output providers to pass endpoint specs through;
- update default templates, builders, and tests;
- remove legacy `pin = ...` compatibility from output node parsing.

Out of scope:

- designing a rich selector language;
- keeping `pin` as fallback sugar;
- implementing button or radio nodes;
- changing fixture-to-output product semantics;
- changing physical resource claim semantics.

## Current State

### Authored output model

- `lp-core/lpc-model/src/nodes/output/output_def.rs` currently defines
  `OutputDef { pin: ValueSlot<u32>, bindings, options }`.
- `OutputDef::new(pin)` and `OutputDef::pin()` are used in engine tests,
  output sink registration, and project builders.
- `lpa-server` default template still writes:

  ```toml
  kind = "Output"
  pin = 4
  ```

- `ProjectBuilder::output()` stores a numeric default pin and serializes it
  through `OutputDef`.

### Runtime output flow

- `EngineServices::register_output_sink` stores `OutputSinkBinding { pin, ... }`.
- `flush_registered_sinks` opens output channels through
  `OutputProvider::open(pin, byte_count, OutputFormat::Ws2811, options)`.
- `OutputProvider` is still pin-shaped in `lpc-shared::output`.
- `MemoryOutputProvider`, `SyscallOutputProvider`, `Esp32OutputProvider`, and
  test wrapper providers all implement the pin-shaped `OutputProvider` API.

### Hardware capability layer

- `HardwareEndpoint` currently carries:
  - `HardwareEndpointId`,
  - `HardwareEndpointKind`,
  - `driver_id`,
  - `HardwareAddress`,
  - display label,
  - status.
- `HardwareEndpointId` is currently generated from `driver_id:address`.
- `HardwareSystem` opens WS281x/Button/Radio by endpoint id or by
  `HardwareAddress`.
- `VirtualWs281xDriver` exposes WS281x endpoints for GPIO addresses in the
  manifest.
- `Esp32RmtWs281xDriver` exposes one WS281x endpoint backed by GPIO18 and
  `/rmt/ws281x0`.
- The Seeed XIAO ESP32-C6 manifest has board labels including `D10` for
  `/gpio/18`.

### Recent radio work

- Radio now uses shared capability-facing abstractions and root-owned ESP-NOW
  registration.
- That work still opens radio by `/radio/0` address in the diagnostic; this
  plan does not need to convert radio nodes because they do not exist yet.
- The endpoint-spec vocabulary should be suitable for future radio strings such
  as `radio:espnow:0`.

## User Notes That Should Shape The Plan

- Do not keep legacy sugar. Remove numeric `pin` output authoring now.
- Keep the authored format simple and obvious.
- Do not build clever decoding or path-style selectors.
- Preferred format is `cap:driver:<config>`, e.g.:
  - `ws281x:rmt:D10`
  - later maybe `ws281x:rmt:D10_D9`
- `config` is terse and driver-owned. Exact multi-pin conventions can be
  decided later.
- The authored string identifies a driver-provided endpoint, not a low-level
  manifest resource.

## Open Questions

No blocking questions.

### Suggested Defaults For This Plan

- Name the type `HardwareEndpointSpec`.
- Store it in `lpc-shared::hardware`.
- Parse by splitting on `:` and requiring exactly three non-empty ASCII parts.
- Keep the config payload opaque to `HardwareSystem`; only drivers construct
  and match specs.
- Make `HardwareEndpoint` carry a `spec: HardwareEndpointSpec`.
- Make endpoint ids derive from the spec initially, or keep ids internal while
  exact spec matching is the authored reopen path.
- For board-label resolution, keep it driver-owned but simple:
  - `VirtualWs281xDriver` should produce specs from manifest display labels or
    aliases for GPIO resources.
  - `Esp32RmtWs281xDriver` should initially produce `ws281x:rmt:D10` for its
    known GPIO18/XIAO output endpoint.
- Use `endpoint = "ws281x:rmt:D10"` in new output TOML and generated templates.
- Replace `OutputProvider::open(pin, ...)` with
  `OutputProvider::open(endpoint, ...)`.

## Validation Notes

The final plan should include:

```bash
cargo fmt --check
cargo test -p lpc-model output
cargo test -p lpc-shared hardware
cargo test -p lpc-shared output
cargo check -p lpc-shared --no-default-features
cargo test -p lpc-engine engine_services
cargo test -p lpc-engine output_flush
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

