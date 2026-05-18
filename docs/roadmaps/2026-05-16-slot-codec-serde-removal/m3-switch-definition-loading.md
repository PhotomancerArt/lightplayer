# Milestone 3: Switch Definition Loading

## Title And Goal

Move authored definitions and artifact/project loading from Serde TOML to
SlotCodec TOML.

## Suggested Plan Location

`docs/roadmaps/2026-05-16-slot-codec-serde-removal/m3-switch-definition-loading/`

## Scope

In scope:

- replace `NodeDef::from_toml_str` serde probing with slot-reader
  discriminator logic
- route project/node/texture/shader/output/fixture definition loading through
  the slot registry
- preserve or intentionally update authored TOML syntax
- migrate serde TOML tests to slot codec TOML tests

Out of scope:

- removing serde derives immediately
- schema versioning or backward compatibility for old TOML
- broad authored format redesign beyond what SlotCodec requires

## Key Decisions

- Discriminators are explicit.
- Unknown fields are errors until schema versioning exists.
- `EnumSlot<T>` handles structured enum payloads during TOML loading.

## Deliverables

- Authored definition loading no longer calls serde for model payloads.
- Definition TOML round-trip/read tests use the slot registry.
- Any syntax deviations from current authored TOML are documented.

## Dependencies

- M1 enum cleanup.
- Useful lessons or helpers from M2 message path migration.

## Execution Strategy

Full plan. This milestone touches authored storage, discriminators, semantic
leaf syntax, and artifact loading.
