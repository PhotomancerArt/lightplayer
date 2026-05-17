# Milestone 3: Reshape The Real Domain

## Title and goal

Reshape production domain types so persisted and wire-visible data fit the
validated slot-native serialization model.

## Suggested plan location

`docs/roadmaps/2026-05-13-slot-native-streaming-serialization/m3-reshape-domain/`

## Scope

In scope:

- Convert remaining persisted concepts into slot roots, slot records, slot
  enums, slot maps, slot options, or semantic slot leaves.
- Replace ambiguous `#[slot(skip)]` usage with explicit model concepts.
- Introduce `#[slot(transient)]` where needed for data that is wire-visible but
  not authored-storage-visible.
- Model `NodeDef` as the thin one-level enum/wrapper needed for loading.
- Revisit `BindingEndpoint` toward `Ref(BindingRef)` / `Value(LpValue)` if the
  validated enum storage supports it.
- Upgrade authored files/tests forward instead of preserving legacy aliases in
  the generic codec.

Out of scope:

- Full custom loader/message adoption.
- Removing Serde derives wholesale.
- Schema migration/versioning.

## Key decisions

- Concrete node definitions remain canonical slot roots.
- Loader discriminators belong in wrappers/envelopes, not hidden skipped fields.
- Unknown fields remain errors.
- Normal enum discriminators use PascalCase variants unless a compact
  single-value enum style is explicitly enabled.

## Deliverables

- Production domain shapes aligned with the mockup-proven model.
- Updated real tests and authored fixtures.
- Documented deviations from the old Serde format that are intentional.
- A clear list of remaining Serde-only policy, if any.

## Dependencies

- Milestone 2 generated mockup proof.
- Agreement on storage metadata and enum wrapper shape.

## Execution strategy

Full plan: this is real model churn across several crates and should be split
into small, reviewable phases.

