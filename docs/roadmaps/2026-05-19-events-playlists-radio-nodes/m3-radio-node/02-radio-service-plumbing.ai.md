# Phase 2: Radio Service Plumbing

## Scope Of Phase

Expose radio hardware to runtime nodes through the same service pattern used by button nodes.

In scope:

- Add `RadioService` in engine services.
- Implement `RadioService` for `HardwareSystem`.
- Add `open_radio_by_spec` to `HardwareSystem` if needed.
- Pass the radio service through `EngineServices`, `TickContext`, and engine tick paths.
- Wire `lpa-server` project new/reload to preserve the radio service.
- Add focused tests for service access and endpoint-spec opening.

Out of scope:

- `ControlRadioNode` runtime behavior.
- Project loader support for `NodeDef::ControlRadio`.
- Example projects.

## Code Organization Reminders

- Keep service traits near `ButtonService` in `engine_services.rs`.
- Keep hardware endpoint helpers in `hardware_system.rs`.
- Put tests at the bottom of the files they exercise.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Update:

- `lp-core/lpc-shared/src/hardware/hardware_system.rs`
  - add `open_radio_by_spec(&HardwareEndpointSpec, RadioConfig)`;
  - use the existing `endpoint_for_spec` helper like button/ws281x.
- `lp-core/lpc-engine/src/engine/engine_services.rs`
  - add `RadioService`;
  - implement for `HardwareSystem`;
  - add `radio_service` storage, setter, and getter.
- `lp-core/lpc-engine/src/node/contexts.rs`
  - add `radio_service: Option<Rc<dyn RadioService>>`;
  - expose `radio_service()`;
  - update constructors to accept/pass the new service.
- `lp-core/lpc-engine/src/engine/engine.rs`
  - pass `host.radio_service.clone()` into tick contexts in both tick paths.
- `lp-app/lpa-server/src/project.rs`
  - add a radio service field only if the surrounding server/root already owns one;
  - otherwise preserve the existing constructor shape and make this phase a smaller engine-only
    plumbing pass.

Important: do not couple `ControlRadioNode` directly to ESP-NOW types. The node should see only
`RadioService` and `RadioDevice`.

Tests:

- `HardwareSystem::open_radio_by_spec` opens `radio:virtual:0` with virtual drivers.
- A second radio open still reports endpoint unavailable while the first handle is alive.
- `TickContext::radio_service()` returns the service passed by `EngineServices`.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-shared virtual_radio
cargo test -p lpc-engine engine_services
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```
