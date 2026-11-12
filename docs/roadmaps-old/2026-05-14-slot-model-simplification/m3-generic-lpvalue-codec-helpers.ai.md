# Milestone 3: Generic LpValue Codec Helpers

## Title And Goal

Add generic syntax helpers that read and write `LpValue` using slot value
metadata.

## Suggested Plan Location

`docs/roadmaps/2026-05-14-slot-model-simplification/m3-generic-lpvalue-codec-helpers/`

## Scope

In scope:

- Read/write primitive and structured `LpValue` forms through `SlotValueShape`
  / `LpType`.
- Keep helpers in `lpc-model/src/slot_codec`.
- Exercise helpers through mockup tests.

Out of scope:

- Per-record codec generation.
- Fully custom enum/discriminator policy.

## Key Decisions

- Normal leaf codec behavior is generic.
- Semantic conversion is owned by `ToLpValue` / `FromLpValue`.

## Deliverables

- Shared helper API for generated record codecs.
- Tests covering representative scalar, struct, array, and semantic value
  shapes.

## Dependencies

Milestone 1.

## Execution Strategy

Full plan. This is the reusable core the generator will call.
