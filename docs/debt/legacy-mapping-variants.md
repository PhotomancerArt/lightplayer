---
status: carried
since: 2026-07-27
logged: 2026-07-28
area: fixture-mapping
related:
  - docs/adr/2026-07-27-map2d-document-architecture.md
  - Planning/lp2025/2026-07-27-2d-mapping-system (plan D1, M5 deliverable 5)
---
# Legacy mapping variants ride along beside Map2d

**Shape** — `MappingConfig` in `lp-core/lpc-model/src/nodes/fixture/mapping.rs`
still carries the three pre-Map2d variants — `PathPoints`, `RingArray`,
`SvgPath` — alongside the `Map2d` document variant that superseded them
(2D mapping plan, D1). Every consumer of the enum (engine loader, slot
projection, examples validation, device resolution) must keep matching
and supporting all four arms, and the legacy resolvers stay compiled
into device firmware. This is a condition, not a bug: the variants work,
but they are a parallel mapping system we no longer author.

**Carrying cost** — ~100 references across the workspace; four-arm
matches in every mapping touchpoint; legacy resolver code on the flash
budget; a second semantics to reason about whenever mapping behavior
changes; new mapping features (editor, wiring views) silently don't
apply to fixtures still on legacy variants.

**Workarounds** — none needed operationally; new work targets `Map2d`
only. The standalone editor imports SVG by flattening to a `Map2d` doc,
so `SvgPath` is already demoted to an import source.

**Retirement path** — migrate remaining example/user projects to
`fixture.map2d.json` documents (the fyeah example migrated in M2),
write a DTO-level project migration for the three variants, then delete
the arms and their resolvers. Alpha share-envelope posture applies:
version + refuse, never silently migrate shared envelopes.
