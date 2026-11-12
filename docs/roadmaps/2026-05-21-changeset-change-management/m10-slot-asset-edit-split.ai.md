# M10 — SlotEdit / AssetEdit Split

Split `EditOp` into `SlotEdit` + `AssetEdit` and make `ArtifactEdit` a tagged
union. Prerequisite for clean asset-side evolution (partial diffs later).

**Plan:** [`00-design.ai.md`](00-design.ai.md) | **Notes:** [`00-notes.ai.md`](00-notes.ai.md)

## Phases

| # | File | Summary |
|---|------|---------|
| 01 | [01-edit-types-serde.ai.md](01-edit-types-serde.ai.md) | New enums, delete `edit_op.rs`, serde tests |
| 02 | [02-apply-pipeline.ai.md](02-apply-pipeline.ai.md) | `apply.rs`, `slot_apply`, registry dispatch |
| 03 | [03-diff-project-diff.ai.md](03-diff-project-diff.ai.md) | `def_diff`, `project_diff` return shapes |
| 04 | [04-tests-and-docs.ai.md](04-tests-and-docs.ai.md) | Harness tests, `change-language.ai.md` |
| 05 | [05-cleanup-validation.ai.md](05-cleanup-validation.ai.md) | fmt, clippy, CI gate, summary |

**Parallel:** 02 ∥ 03 after 01.

## Status

Complete — types, apply, diff, tests, and docs migrated.
