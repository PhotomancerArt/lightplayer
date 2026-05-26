# Milestone 8: Cleanup + Validation

## Title and goal

Remove deprecated paths, **delete legacy mutation stack**, CI gate, summary.

## Suggested plan location

`docs/roadmaps/2026-05-21-engine-registry-cutover/m8-cleanup-validation/`

## Scope

**In:**

- Execute **`mutation-inventory.md`** from M1 (grep-clean):
  - `lpc-wire` slot mutation types
  - `lpc-view` `prepare_set_value` / pending queue
  - `lpc-engine` `slot_mutation.rs`
  - `lpa-server` mutation dispatch
  - `lp-cli` debug UI mutation queue (replace with edit sync + commit UX)
- Old `lpc-engine` artifact module (if any remnants post-M4)
- `just ci`; fw-esp32 check; roadmap summaries

**Out:** M10 provenance probes.

## Dependencies

- New edit path working end-to-end (M5 minimum)

## Execution strategy

**Full plan** with checklist derived from M1 inventory.
