# M3.1 Authored Endpoint Specs Design

## Scope Of Work

Replace output node numeric pin authoring with a required driver-provided
hardware endpoint spec string.

The authored format is intentionally simple:

```text
cap:driver:config
```

Initial shipped output example:

```toml
kind = "Output"
endpoint = "ws281x:rmt:D10"
```

This milestone does not preserve `pin = ...` compatibility for output nodes.

## File Structure

```text
lp-core/lpc-shared/src/hardware/
  hardware_endpoint_spec.rs
  hardware_endpoint.rs
  hardware_system.rs
  virtual_ws281x_driver.rs
  ws281x_driver.rs

lp-core/lpc-shared/src/output/
  provider.rs
  memory.rs

lp-core/lpc-model/src/nodes/output/
  output_def.rs

lp-core/lpc-engine/src/engine/
  engine_services.rs
  output_flush_tests.rs

lp-fw/fw-emu/src/
  output.rs

lp-fw/fw-esp32/src/output/
  provider.rs
  rmt_ws281x_driver.rs

lp-app/lpa-server/src/
  template.rs
  project.rs

lp-core/lpc-shared/src/project/
  builder.rs
```

## Architecture Summary

The authored endpoint spec names a selectable driver endpoint, not a physical
manifest resource. Drivers generate exact endpoint specs for the endpoints they
can open. `HardwareSystem` matches those specs exactly and delegates to the
owning driver.

The node and output flush path carry the endpoint string through unchanged:

```text
Output TOML endpoint
        |
        v
OutputDef.endpoint
        |
        v
EngineServices output sink binding
        |
        v
OutputProvider::open(endpoint, ...)
        |
        v
HardwareSystem::open_ws281x_by_spec(...)
        |
        v
Driver opens endpoint and claims concrete resources
```

Low-level claims still use concrete `HardwareAddress` values such as
`/gpio/18` and `/rmt/ws281x0`.

## Main Components And Interactions

### HardwareEndpointSpec

`HardwareEndpointSpec` is a tiny `no_std + alloc` value in `lpc-shared`:

- parses exactly three non-empty parts separated by `:`;
- exposes `capability()`, `driver()`, `config()`, and `as_str()`;
- supports construction from static/owned strings;
- implements display and serde.

It does not interpret the config payload.

### HardwareEndpoint

`HardwareEndpoint` gains a `spec: HardwareEndpointSpec`.

Drivers are responsible for generating specs they support:

- Virtual WS281x: `ws281x:rmt:<label-or-address-config>`.
- ESP32 RMT WS281x: `ws281x:rmt:D10`.

The existing endpoint id can remain as an internal open token. Authored reopen
uses exact spec matching.

### HardwareSystem

`HardwareSystem` gains:

```rust
open_ws281x_by_spec(&HardwareEndpointSpec, Ws281xConfig)
```

The method searches `ws281x_endpoints()`, matches `endpoint.spec() == spec`, and
opens that endpoint. If the only match is unavailable, it returns the driver's
normal endpoint-unavailable error. If no match exists, it returns an unknown
endpoint error using the spec text as the endpoint id for diagnostics.

Address-based open helpers may remain for lower-level tests and diagnostics, but
output node execution should stop using them.

### OutputDef

`OutputDef` becomes:

```rust
pub struct OutputDef {
    pub endpoint: ValueSlot<HardwareEndpointSpec>,
    pub bindings: BindingDefs,
    pub options: OptionSlot<OutputDriverOptionsConfig>,
}
```

`pin` is removed. `pin = ...` TOML should no longer parse as an output node.

Because `lpc-model` should not depend on `lpc-shared`, the endpoint spec type
should live in `lpc-model` if dependency direction requires it, and `lpc-shared`
should re-export or reuse that type. If that is simpler during implementation,
name the model-owned type `HardwareEndpointSpec` and put it under
`lpc-model/src/hardware_endpoint_spec.rs`.

### OutputProvider

`OutputProvider::open` changes from:

```rust
open(pin: u32, byte_count, format, options)
```

to:

```rust
open(endpoint: &HardwareEndpointSpec, byte_count, format, options)
```

Memory, emu, ESP32, and test wrapper providers should pass the spec to
`HardwareSystem::open_ws281x_by_spec`.

### Defaults

Default generated projects should write:

```toml
kind = "Output"
endpoint = "ws281x:rmt:D10"
```

Test builders should default to the same endpoint unless a test overrides it.

