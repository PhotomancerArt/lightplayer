# M10 — SlotEdit / AssetEdit Split

Split `EditOp` into `SlotEdit` + `AssetEdit` and make `ArtifactEdit` a tagged
union. Prerequisite for clean asset-side evolution (partial diffs later).

**Plan:** [`00-design.md`](00-design.md) | **Notes:** [`00-notes.md`](00-notes.md)

## Phases

| # | File | Summary |
|---|------|---------|
| 01 | [01-edit-types-serde.md](01-edit-types-serde.md) | New enums, delete `edit_op.rs`, serde tests |
| 02 | [02-apply-pipeline.md](02-apply-pipeline.md) | `apply.rs`, `slot_apply`, registry dispatch |
| 03 | [03-diff-project-diff.md](03-diff-project-diff.md) | `def_diff`, `project_diff` return shapes |
| 04 | [04-tests-and-docs.md](04-tests-and-docs.md) | Harness tests, `change-language.md` |
| 05 | [05-cleanup-validation.md](05-cleanup-validation.md) | fmt, clippy, CI gate, summary |

**Parallel:** 02 ∥ 03 after 01.

## Status

Complete — types, apply, diff, tests, and docs migrated.
