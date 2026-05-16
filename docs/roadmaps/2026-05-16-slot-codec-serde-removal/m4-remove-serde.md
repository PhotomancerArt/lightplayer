# Milestone 4: Remove Serde From `lpc-model`

## Title And Goal

Delete remaining Serde derives, helpers, tests, and dependencies from
`lpc-model`.

## Suggested Plan Location

`docs/roadmaps/2026-05-16-slot-codec-serde-removal/m4-remove-serde/`

## Scope

In scope:

- remove serde derives and manual serde impls from `lpc-model`
- replace or delete serde-only tests
- provide non-serde codecs or debug/snapshot paths for slot infrastructure
  where still needed
- remove `serde` and `serde_json` from `lpc-model/Cargo.toml`
- remove stale serde-only helper APIs and annotations once slot paths own
  read/write behavior
- update or delete old serde-era authored syntax tests and fixtures inside
  `lpc-model`
- update docs to remove transitional language about serde-backed model paths
- update docs after the migration is complete

Out of scope:

- removing serde from `lpc-wire` or other crates unless required by compile
  boundaries
- changing schema versioning policy
- project-builder/authored project writing migration; that should land as its
  own step before the final serde deletion
- broad `NodeDef` API reshaping; keep only final naming polish if needed

## Key Decisions

- Serde removal happens after real JSON and TOML paths already use SlotCodec.
- Slot infrastructure types may need purpose-built snapshot codecs instead of
  generic serde derives.

## Deliverables

- `lpc-model` compiles without a direct serde dependency.
- Tests no longer use `serde_json` inside `lpc-model`.
- Final docs describe the post-migration state.

## Dependencies

- M2 message paths switched.
- M3 definition loading switched.

## Execution Strategy

Full plan. This is the final cleanup and validation milestone for the whole
effort.
