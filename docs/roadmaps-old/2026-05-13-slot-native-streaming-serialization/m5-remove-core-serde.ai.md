# Milestone 5: Remove Core Serde

## Title and goal

Remove Serde from the `no_std` core parts of the project after slot-native
serialization owns the domain paths.

## Suggested plan location

`docs/roadmaps/2026-05-13-slot-native-streaming-serialization/m5-remove-core-serde/`

## Scope

In scope:

- Audit remaining `serde::{Serialize, Deserialize}` derives in no-std core
  crates.
- Remove Serde from migrated domain models.
- Keep or isolate Serde in host-only tooling, schema generation, tests, or
  compatibility adapters when justified.
- Update feature flags so embedded core paths no longer pull broad Serde
  serialization code.
- Run targeted host and RV32 validation.

Out of scope:

- Removing Serde from every host-only tool unconditionally.
- Removing third-party parsers that use Serde internally if they are isolated
  off the embedded hot path.

## Key decisions

- `std` means host conveniences, not "has serialization."
- Removing Serde must not remove or gate the on-device compiler path.
- Any retained Serde use should be explicit and outside the no-std core product
  path.

## Deliverables

- Serde removed from no-std core domain serialization paths.
- Cargo feature/dependency cleanup.
- Size comparison before and after.
- Final roadmap validation notes.

## Dependencies

- Milestone 4 production custom serialization adoption.
- A stable answer for host tooling and schema snapshots.

## Execution strategy

Full plan: dependency removal can easily affect features and targets, so it
should be planned, measured, and validated carefully.
