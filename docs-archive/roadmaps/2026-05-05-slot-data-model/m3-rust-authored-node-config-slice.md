# Milestone 3: Rust-Authored Node Config Slice

## Title And Goal

Apply the slot model to one simple existing node config as a vertical slice.

## Suggested Plan Location

`docs/roadmaps/2026-05-05-slot-data-model/m3-rust-authored-node-config-slice/`

## Scope

In scope:

- Choose one simple existing node config, likely `TextureDef` or output driver
  options.
- Introduce a Rust-authored config struct shaped naturally for slots.
- Represent config as a `SlotTree` / `SlotData` using manual implementations.
- Keep source file parsing working.
- Add tests showing authored source data can become slot data and back where
  useful.

Out of scope:

- Migrating fixture mapping.
- Proc-macro derives.
- Broad source format changes.
- Runtime mutation or generic wire sync.

## Key Decisions

- Start with a small node because this milestone is about proving the bridge,
  not solving the hardest config.
- Keep graph references and broader `NodeDef` slot modeling out of the first
  vertical slice unless the chosen node requires it.
- Manual implementations are acceptable until the model is proven.

## Deliverables

- One real node config represented through slot data.
- Tests for parse/load compatibility and slot data projection.
- Notes about what a derive macro would remove later.

## Dependencies

- Milestone 1 model foundation.
- Milestone 2 initial shape vocabulary.

## Execution Strategy

Full plan. Even a small vertical slice crosses model/source boundaries and
should capture decisions before editing.

