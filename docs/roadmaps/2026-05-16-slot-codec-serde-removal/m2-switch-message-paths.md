# Milestone 2: Switch Message Paths

## Title And Goal

Move the first real JSON/message serialization path from Serde to SlotCodec.

## Suggested Plan Location

`docs/roadmaps/2026-05-16-slot-codec-serde-removal/m2-switch-message-paths/`

## Scope

In scope:

- identify the smallest real message payload path currently using serde for
  model data
- switch that payload to registry-backed slot JSON read/write
- leave serde derives and annotations in place temporarily
- update or add tests to prove the message path no longer depends on serde

Out of scope:

- removing serde from `lpc-model`
- changing public message envelopes unless needed to carry slot payloads
- switching authored TOML/project loading

## Key Decisions

- This milestone proves the wire side first.
- Existing serde helpers may remain if no call path uses them for the migrated
  message payload.
- JSON syntax should come from `SlotWriter` / `JsonSyntaxSource`, not a
  `SlotData` intermediary.

## Deliverables

- One production message path serializes/deserializes model payloads through
  SlotCodec.
- Tests fail if that path regresses to serde.
- Any missing semantic leaf JSON codecs needed by the chosen message path are
  added in `lpc-model/src/slot_codec`.

## Dependencies

- M1 enum cleanup.

## Execution Strategy

Full plan. The message boundary may involve crates outside `lpc-model`, so the
exact path and validation commands should be pinned before editing.
