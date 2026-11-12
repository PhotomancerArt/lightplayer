# Milestone 1: Output Resource Ownership

## Title And Goal

Add a small hardware registry and make LED outputs claim GPIO/RMT resources so dynamic output pin
selection and conflicts behave cleanly.

## Suggested Plan Location

`docs/roadmaps/2026-05-18-firmware-hardware-io/m1-output-resource-ownership/`

## Scope

In scope:

- Shared `no_std + alloc` hardware address, manifest, resource, claim, lease, and error types.
- A memory/virtual hardware registry for tests and `fw-emu`.
- An ESP32-C6 board manifest with GPIO metadata, known dangerous/reserved pins, and RMT resource
  metadata.
- Output provider integration so opening a WS281x output claims GPIO plus the current RMT channel.
- Clear errors for duplicate pins, reserved pins, unsupported capabilities, and RMT contention.
- Backward compatibility for authored `pin = 18`.
- Tests for two outputs on the same pin and two outputs on different pins contending for RMT.

Out of scope:

- Multi-RMT-channel LED output.
- Full board database or dynamic board config file loading.
- Final UI for pin scanning/dropdowns.
- GPIO input support.

## Key Decisions

- Hardware registry is separate from drivers.
- Claims are atomic bundles, not one-resource-at-a-time best effort.
- String-capable addresses are the target model; numeric pins remain compatibility input.
- `lpc-shared::hardware` is the initial home unless implementation proves it needs its own crate.

## Deliverables

- Shared hardware model and memory registry.
- ESP32-C6 manifest module.
- Output provider claims/releases hardware leases.
- Engine/provider tests cover graceful output conflicts.
- `fw-emu` can represent virtual GPIO/RMT resources.

## Dependencies

- Existing output provider and engine sink flush path.
- Existing ESP32 RMT driver.

## Execution Strategy

Small plan. The direction is clear, but the ESP32 HAL ownership details and migration path from
`pin: u32` deserve a short plan before editing production firmware.
