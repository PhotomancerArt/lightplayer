# Milestone 2: GPIO Button Input

## Title And Goal

Support GPIO button inputs using internal pull-up and button-to-ground wiring, producing debounced
button events that firmware and project logic can consume.

## Suggested Plan Location

`docs/roadmaps/2026-05-18-firmware-hardware-io/m2-gpio-button-input/`

## Scope

In scope:

- A GPIO input/button resource claim using the shared hardware registry.
- ESP32-C6 GPIO input setup with internal pull-up.
- A small debouncer suitable for a big physical button.
- A simple event type for press/release or press-only behavior.
- `fw-emu` support for virtual button events.
- Tests for pin conflicts between output and button users.

Out of scope:

- Matrix keyboards, rotary encoders, capacitive touch, or complex input devices.
- User-facing playlist/event switching implementation.
- Wireless transport; this milestone only produces local events.

## Key Decisions

- The first supported wiring pattern is internal pull-up with a normally-open button to GND.
- Button input claims the GPIO, so it cannot silently share pins with LED output.
- Debouncing is part of the button input module, not project logic.
- Event payloads should be small and radio-friendly, anticipating Milestone 3.

## Deliverables

- Shared or firmware-local `ButtonEvent` type.
- ESP32 button input module.
- Emulated button source for tests.
- Conflict tests with output resources.
- A firmware test or diagnostic path that logs button presses.

## Dependencies

- Milestone 1 hardware registry and GPIO claim support.

## Execution Strategy

Small plan. The hardware behavior is simple, but event shape and async integration should be written
down before it is threaded into the firmware loop.
