# Milestone 6: Legacy Detail Removal

## Title And Goal

Remove legacy node detail/state projection after slot sync reaches parity.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m6-legacy-detail-removal/`

## Scope

In scope:

- Remove or quarantine legacy `NodeState` detail projection from the active project sync path.
- Delete compatibility projection hooks that are replaced by slot roots.
- Remove obsolete prop/view exports and old detail request paths.
- Shrink or delete `lpc-slot-mockup` once production tests cover the same behavior.
- Migrate remaining examples/tests that still depend on legacy detail objects.

Out of scope:

- Large engine API cleanup unrelated to the detail bridge.
- Client-driven mutation.
- New node features.

## Key Decisions

- Bridge code must be removed once parity exists.
- Any retained compatibility code must be explicitly marked legacy with a concrete reason.
- The mockup is a pressure harness, not permanent product code.

## Deliverables

- Active project sync no longer requires legacy `NodeState` details.
- Node-specific wire shapes are removed from the main path.
- Obsolete tests are migrated or deleted.
- Remaining legacy modules are documented as intentionally retained or scheduled for deletion.

## Dependencies

- Milestone 5 generic UI parity.

## Execution Strategy

Full plan. Removal work is easy to underestimate and should be staged so regressions are obvious.

