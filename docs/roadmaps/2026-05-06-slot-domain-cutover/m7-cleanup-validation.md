# Milestone 7: Cleanup And Validation

## Title And Goal

Finalize the cutover with integration validation, documentation, and cleanup of temporary scaffolding.

## Suggested Plan Location

`docs/roadmaps/2026-05-06-slot-domain-cutover/m7-cleanup-validation/`

## Scope

In scope:

- Run broad host and RV32 validation appropriate for the touched crates.
- Validate examples, with `examples/basic` as the canonical project.
- Update architecture docs and code rustdocs for slot roots, watching, resources, and project sync.
- Remove temporary TODOs or move them into explicit future work.
- Audit crate exports for old vocabulary leaks.
- Confirm low-bandwidth resource behavior.

Out of scope:

- Client-driven mutation.
- Engine mutation cleanup.
- New UI product work.

## Key Decisions

- This milestone is required, not optional.
- The roadmap is not complete until temporary bridge code and mockup leftovers are either gone or explicitly justified.

## Deliverables

- Passing validation commands documented in the plan for this milestone.
- Updated examples and docs.
- Final cleanup diff removing temporary bridge/scaffold residue.
- A concise list of remaining future work.

## Dependencies

- Milestone 6 legacy detail removal.

## Execution Strategy

Full plan. The final sweep spans many crates and should be explicit about validation and cleanup boundaries.

