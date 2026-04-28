# Phase 7 — Cleanup, Summary, And Validation

## Scope of Phase

Review the complete Milestone 2 implementation, remove temporary code, run final
validation, and write the plan summary.

Out of scope:

- Do not implement new feature work beyond small fixes needed to make the
  already-planned M2 work correct.
- Do not commit. The main agent will commit after reviewing the final diff.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

## Cleanup & Validation

Review the git diff for:

- Temporary comments or TODOs introduced by the plan.
- Debug prints.
- `dbg!`, `println!`, `eprintln!` added for debugging.
- Unnecessary `#[allow(...)]` additions.
- Disabled, skipped, or weakened tests.
- Over-broad refactors unrelated to texture filetests.
- Public names that still call `LpsTexture2DDescriptor` a "uniform" descriptor.
- Any use of `UVec4` as a texture descriptor stand-in.

Run formatting:

```bash
cargo +nightly fmt
```

Run targeted validation:

```bash
cargo test -p lpvm set_uniform
cargo test -p lps-filetests texture
cargo test -p lps-filetests --test filetests -- --ignored --nocapture
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If time permits, run the broader local check:

```bash
just test-filetests
```

Do not run `cargo test --workspace`; this repo has RV32-only workspace members
that intentionally do not build for the host target.

## Plan Cleanup

Create:

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m2-texture-filetests/summary.md`

The summary should have:

```markdown
# Milestone 2 — Texture filetests (summary)

## What was built

- ...

## Decisions for future reference

#### <short title>

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Capture only decisions that future readers might relitigate. Suggested decision
candidates:

- Runtime texture descriptors use normal typed `set_uniform` with
  `LpsValueF32/Q32::Texture2D`; raw `UVec4` stand-ins remain rejected.
- Inline fixtures are file-level in M2; per-run fixtures and sidecars are
  deferred.
- Exact hex channel width follows storage format; M2 uses 4 hex digits because
  supported formats are unorm16.

Because this plan directory is under `docs/roadmaps/...`, do not archive it to
`docs/plans-old/`.

## Final Report

Report back:

- Summary of cleanup changes made.
- Validation commands run and whether they passed.
- Any commands not run and why.
- Any residual risks or follow-up items.

