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
- update docs after the migration is complete

Out of scope:

- removing serde from `lpc-wire` or other crates unless required by compile
  boundaries
- changing schema versioning policy

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
