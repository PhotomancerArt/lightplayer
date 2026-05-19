# Milestone 1: Control Message And Trigger Event

## Title And Goal

Define the minimal slotted control-message/event envelope that can be routed on the bus.

## Suggested Plan Location

`docs/roadmaps/2026-05-19-events-playlists-radio-nodes/m1-control-message-trigger-event/`

## Scope

In scope:

- Add `ControlMessage` with `id` and `seq`.
- Expose `TriggerEvent` as the first no-args one-shot event family.
- Prove the message can move through `bus#trigger`.
- Prove compute and visual shaders can consume sentinel-mapped maps through shared input code.
- Add a checked-in example project showing the end-to-end flow.
- Document deferred OSC-compatible address and args.

Out of scope:

- Button, playlist, or radio nodes.
- OSC network interoperability.
- MIDI/DMX bridges.
- Payload args, bundles, timetags, or pattern routing.

## Key Decisions

- Use `ControlMessage` for the long-term control-data broker domain.
- Keep `TriggerEvent` as the first tiny semantic event family.
- Route first-slice triggers by bus channel; omit explicit address for now.
- Omit `source` for M1 so the shape stays directly GLSL-compatible.
- Use existing `u32` IDs and sequences for M1 rather than widening the model value grammar.
- Use `id` as the map key field to match existing sentinel-mapped shader structs like
  `FluidEmitter`.

## Deliverables

- Slotted `ControlMessage` / `TriggerEvent` model type.
- Bus/resolver tests for `bus#trigger` message routing.
- Shared compute/visual consumed-map support for fixed sentinel-mapped event arrays.
- `examples/trigger-events` with two compute event producers and a visual shader that draws up to
  eight event circles.
- Plan summary and validation notes.

## Dependencies

- Existing bus/binding system.
- Existing slotted model infrastructure.
- Existing endpoint/hardware roadmap work is not directly required for M1.

## Execution Strategy

Full plan. This touches shared model vocabulary plus bus behavior, and the field set is deliberately
small enough that phase boundaries should keep it controlled.
