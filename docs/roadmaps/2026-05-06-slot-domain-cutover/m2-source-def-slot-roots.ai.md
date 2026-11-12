# Milestone 2: Source Def Slot Roots

## Title And Goal

Expose real TOML-authored node definitions as production slot roots.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m2-source-def-slot-roots/`

## Scope

In scope:

- Implement `StaticSlotAccess` for real source definitions:
  - `ProjectDef`
  - `TextureDef`
  - `ShaderDef`
  - `OutputDef`
  - `FixtureDef`
- Register source shapes in a production shape registry.
- Add source-slot tests from actual TOML examples, starting with `examples/basic`.
- Add shader `param_defs` if this is the right time to bring the mockup concept into the real source model.
- Ensure source refs, paths, enums, options, maps, and semantic leaf types are represented cleanly.

Out of scope:

- Runtime node state exposure.
- Replacing project wire sync.
- Full client mutation or artifact mutation.
- Migrating every example at the beginning of the milestone.

## Key Decisions

- Source defs implement slot access directly; do not force an artificial `config` wrapper unless a node's shape truly benefits from it.
- Source shapes are static Rust-authored shapes where possible.
- Dynamic authored shader params use map/record shapes with stable keys.

## Deliverables

- Source defs can be traversed generically through `SlotAccess`.
- Shape registration covers all existing core node defs.
- Tests print or assert generic walks over real source defs.
- `examples/basic` remains the canonical source fixture for this milestone.

## Dependencies

- Milestone 1 root naming and metadata conventions.

## Execution Strategy

Full plan. The source model is broad and must be migrated carefully without obscuring the domain decisions.

