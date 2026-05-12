# Phase 5: Docs, Cleanup, And Validation

## Scope Of Phase

Clean up M1.3, document the registration model, and run final validation.

In scope:

- Document static vs dynamic shape registration semantics.
- Update M2 notes to say source conversion should use generated bootstrap.
- Remove stale manual registration helpers if they are no longer needed.
- Search for TODOs, debug output, and stale generated artifacts.
- Run final validation.

Out of scope:

- Source def cutover.
- Engine runtime slot roots.
- CI push/watch unless separately requested.

## Code Organization Reminders

- Put durable docs near the relevant model/codegen APIs.
- Keep plan summary in this M1.3 directory.
- Do not leave generated `OUT_DIR` files in the repo.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests.
- If blocked, stop and report.
- Report changed files, validation, and deviations.

## Implementation Details

Audit:

```bash
git status --short
rg "register_shapes\\(|register_shape\\(" lp-core/lpc-slot-mockup lp-core/lpc-model lp-core/lpc-slot-macros
rg "TODO|dbg!" lp-core/lpc-model lp-core/lpc-slot-macros lp-core/lpc-slot-codegen lp-core/lpc-slot-mockup
```

Expected docs:

- `StaticSlotShape` rustdocs explain type-owned static root shape semantics.
- `StaticSlotAccess` rustdocs explain data-access roots.
- Registry rustdocs explain `register_tree` versus `ensure_tree` versus
  `replace_tree`.
- Codegen docs explain:
  - `OUT_DIR` output,
  - discovery rules,
  - root-only registration,
  - dynamic shapes are not generated.

Write:

- `docs/roadmaps/2026-05-06-slot-domain-cutover/m1.3-slot-shape-registration-codegen/summary.md`

Final validation:

```bash
cargo fmt --check --package lpc-model --package lpc-slot-macros --package lpc-slot-codegen --package lpc-slot-mockup
cargo test -p lpc-model --lib --tests
cargo test -p lpc-model --features derive --test slot_record_derive
cargo test -p lpc-slot-codegen
cargo test -p lpc-slot-mockup -- --nocapture
cargo check -p lpc-model --features schema-gen
cargo clippy -p lpc-model --all-targets -- -D warnings
cargo clippy -p lpc-slot-macros --all-targets -- -D warnings
cargo clippy -p lpc-slot-codegen --all-targets -- -D warnings
cargo clippy -p lpc-slot-mockup --all-targets -- -D warnings
git diff --check
```
