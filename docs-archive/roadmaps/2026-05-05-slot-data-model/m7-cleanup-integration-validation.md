# Milestone 7: Cleanup And Integration Validation

## Title And Goal

Remove temporary scaffolding and validate the slot data model across the domain.

## Suggested Plan Location

`docs/roadmaps/2026-05-05-slot-data-model/m7-cleanup-integration-validation/`

## Scope

In scope:

- Remove stale compatibility types and adapters made obsolete by the migration.
- Clean up docs and rustdocs so slot data is the canonical vocabulary.
- Run integration validation across model/source/engine/wire/view/server crates.
- Update examples.
- Record follow-up work for artifact mutation and any remaining client/server
  gaps.

Out of scope:

- Building artifact mutation APIs.
- Adding new product features.
- Large shape-vocabulary expansions not required by existing nodes.

## Key Decisions

- Cleanup should happen after the model is applied broadly enough to avoid
  preserving misleading old concepts.
- Final docs should clearly explain slot tree, slot data, slot shape, resources,
  and shader ABI boundaries.

## Deliverables

- Removed temporary compatibility scaffolding.
- Final docs and roadmap summary.
- Green validation commands.
- Follow-up notes for artifact mutation through the message API.

## Dependencies

- Milestone 6 migration across existing nodes/client surfaces.

## Execution Strategy

Full plan. The final cleanup spans multiple crates and should be validated as a
coherent integration pass.

