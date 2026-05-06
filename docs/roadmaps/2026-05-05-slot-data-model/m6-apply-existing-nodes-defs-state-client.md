# Milestone 6: Apply Existing Nodes, Defs, State, And Client

## Title And Goal

Migrate the existing node/domain surfaces to the slot data model.

## Suggested Plan Location

`docs/roadmaps/2026-05-05-slot-data-model/m6-apply-existing-nodes-defs-state-client/`

## Scope

In scope:

- Apply slot-shaped config/state/param modeling across existing core nodes.
- Rework source defs where appropriate to expose slot-shaped authoring data.
- Move resource references into generic slot data where possible.
- Update client/view caches to understand slot trees and slot versions.
- Replace narrow legacy state objects where the slot model can cover them.
- Use derive support from Milestone 5 where it reduces mechanical code.
- Keep examples and integration tests moving as migration proceeds.

Out of scope:

- Server-side artifact mutation through the message API.
- Complete removal of every compatibility adapter if doing so would block
  incremental migration.
- New node types.

## Key Decisions

- This is the milestone where the model becomes the normal domain vocabulary,
  not just a sidecar.
- Fixture mapping should be revisited here because it is the important hard
  config example.
- Existing examples/tests may be migrated near the end of the milestone after
  the model is proven on enough nodes.

## Deliverables

- Existing core node defs/config/state represented through slot data where
  appropriate.
- Client/view projection updated for slot trees.
- Tests covering texture/output/shader/fixture paths.
- Updated documentation and examples.

## Dependencies

- Milestone 5 slot derive authoring.

## Execution Strategy

Full plan. This is a broad migration milestone and should be broken into
reviewable phases.

