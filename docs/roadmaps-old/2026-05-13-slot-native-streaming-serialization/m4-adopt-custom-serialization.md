# Milestone 4: Adopt Custom Serialization

## Title and goal

Use the slot-native serializer for production loading and selected wire
messages.

## Suggested plan location

`docs/roadmaps/2026-05-13-slot-native-streaming-serialization/m4-adopt-custom-serialization/`

## Scope

In scope:

- Replace production TOML loading root by root.
- Start with `OutputDef`, then expand through project/source node defs.
- Replace selected JSON message parsing/writing where size or memory matters.
- Route large/resource-like messages through streaming reader/writer paths.
- Keep Serde compatibility only where explicitly still needed.
- Measure host and firmware impact after meaningful adoption.

Out of scope:

- Removing all Serde dependencies.
- Replacing host-only tooling before production paths are stable.
- Protocol compatibility negotiation.

## Key decisions

- Adoption should follow proven root/message slices, not a big-bang rewrite.
- TOML authored loading can stay tree-backed if the semantics are shared.
- JSON wire paths should prefer streaming for large payloads.
- Existing Serde behavior is a comparison oracle during migration, not the
  long-term source of truth.

## Deliverables

- Production loaders using generated slot-native readers.
- Production writers using slot-native output streams.
- Tests comparing old/new behavior where useful.
- Firmware/code-size and allocation notes after adoption slices.
- Removed or isolated Serde usage from migrated paths.

## Dependencies

- Milestone 3 domain shape alignment.
- Generated reader/writer support for production roots.

## Execution strategy

Full plan: production adoption touches loading, messages, tests, and size
measurement, so it needs phased implementation and validation.

