# Milestone 3: Basic Radio Messages

## Title And Goal

Promote the ESP-NOW smoke test into a reusable firmware radio message module with a simple
single-consumer, channel-aware send/drain API.

## Suggested Plan Location

`docs/roadmaps/2026-05-18-firmware-hardware-io/m3-basic-radio-messages/`

## Scope

In scope:

- Move the smoke-test packet/event code out of `fw-esp32/src/tests/test_espnow.rs`.
- Keep a fixed, no-alloc packet format for tiny messages.
- Include LightPlayer packet magic, source device ID, and `u32` channel ID in every packet.
- Provide a single-consumer API to subscribe/unsubscribe to `u32` channel IDs.
- Provide a single-consumer API to send one message on a channel and drain received messages for a
  channel since the last read.
- Ignore packets with the wrong magic and ignore packets for unsubscribed channels.
- Report per-channel receive-buffer overflow/drop state explicitly to the consumer.
- Represent button press messages from Milestone 2 as one initial message kind.
- Preserve de-duplication by source/device/event ID.
- Add a small hardware-facing producer/consumer API; the consumer may later be a node, bus bridge,
  or server/runtime inbox, but that semantic layer is out of scope here.
- Claim the radio resource through the hardware registry when ESP-NOW is active.
- Keep the existing smoke test or replace it with a production-module-backed test.

Out of scope:

- Thread/OpenThread.
- Reliable delivery, pairing UX, encryption, or full mesh routing.
- General wireless bus sync beyond the minimum channel message path.
- LightPlayer event semantics, node design, playlist/event-driven visual switching, or project graph
  integration.
- Multiple independent radio consumers, encryption, pairing, routing, or complex filtering beyond
  exact channel subscription.

## Key Decisions

- ESP-NOW is the first radio transport because the smoke test already worked on two ESP32-C6 boards.
- Packet format remains tiny and fixed-size until a real need appears.
- Radio transport should not depend directly on LightPlayer project state.
- The first API assumes one consumer, but includes exact `u32` channel subscription so devices can
  ignore traffic that is not for them.
- Receive-side buffering is per-channel and must expose an overflow flag so the consumer can know
  messages were dropped.
- Radio dependencies should remain optional until the production firmware intentionally enables them.

## Deliverables

- Reusable ESP-NOW radio message module.
- Shared tiny message packet definitions.
- `subscribe_channel`, `send_channel`, and `drain_channel` style API with per-channel overflow/drop
  reporting.
- Button press message broadcast and receive path.
- De-duplication tests or firmware diagnostics.
- Updated smoke-test report or validation notes.

## Dependencies

- Milestone 2 button message/event source shape.
- Existing ESP-NOW smoke test and `esp-radio` dependency wiring.

## Execution Strategy

Full plan. Radio touches optional dependencies, async firmware flow, packet compatibility, and test
modes; it deserves notes, design, and phased implementation before becoming production code.
