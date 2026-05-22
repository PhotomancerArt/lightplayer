# Milestone 7: Cleanup + Validation

## Title And Goal

Cross-milestone validation, doc updates, and removal of temporary scaffolding.
Confirm parent roadmap cross-links and CI gate for this roadmap.

## Parallel Build

Validation only — no new features. Parent **M6** engine cutover is a **separate**
milestone after M6 diff gate here.

## Suggested Plan Location

`docs/roadmaps/2026-05-21-changeset-change-management/m7-cleanup-validation/`

## Scope

In scope:

- `cargo test -p lpc-node-registry` — all unit, fs-change, changeset, diff tests green
- `cargo clippy -p lpc-node-registry --all-targets --no-deps -- -D warnings`
- Roadmap `summary.md` for this promotion
- Update parent artifact-routed overview / M6 stub with gate status
- Remove dead stubs (`change/mod.rs` placeholder comments, view passthrough notes)
- Public API docs on `ChangeSet`, overlay, commit, `NodeDefView`

Out of scope:

- Parent M6 engine cutover
- Wire ChangeSet protocol ([`future.md`](future.md))
- Parent M10 ExplainSlot probes

## Key Decisions

- CI gate matches AGENTS.md `just check` for touched crates.

## Deliverables

- `docs/roadmaps/2026-05-21-changeset-change-management/summary.md`
- Clean exports in `lpc-node-registry/lib.rs`
- Validation commands recorded in summary

## Dependencies

- M6 diff + equivalence gate green

## Execution Strategy

Small plan. Checklist-driven verification and doc pass.

Suggested chat opener:

> M7 cleanup: validation sweep + summary + parent cross-link update. Agree?
