# Phase 7: Cleanup, Validation, and Summary

## Scope of Phase

Perform final cleanup and validation for the M9 plan, then write the plan
summary. This phase is supervised: a sub-agent may do the first pass, but the
main agent must review the final diff and validation directly.

In scope:

- Remove temporary code, debug prints, accidental TODOs, and stale comments.
- Run formatting and focused validation.
- Run q32 filetests for accepted targets.
- Check for stale `@broken` / `@unsupported` markers in touched files.
- Add `summary.md` to the M9 plan directory.

Out of scope:

- New feature work.
- Solving unrelated filetest failures.
- Moving the plan directory. This is a roadmap milestone under
  `docs/roadmaps/...`, so it must remain in place.
- Committing changes. The main agent handles the final commit only when the
  user explicitly asks.

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
- If something blocks completion, stop and report back rather than
  improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Read first:

- `docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout/00-notes.md`
- `docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout/00-design.md`
- All previous phase reports if available.

Cleanup checklist:

- Search the diff for temporary code:
  - `TODO`
  - `dbg!`
  - `println!`
  - `eprintln!`
  - `panic!`
  - `unimplemented!`
  - `todo!`
  - new `#[allow(...)]`
- Inspect any matches. Remove accidental/debug code. Keep only intentional
  existing comments or justified TODOs that are not part of this plan.
- Ensure helper functions are placed at the bottom of files where consistent
  with local style.
- Ensure `lower_lvalue.rs` has a clear public-in-crate API and does not expose
  implementation details unnecessarily.
- Ensure `lower_call.rs` remains focused on call ABI assembly.
- Ensure no new `jit.q32` markers were added.
- Ensure touched filetests do not have stale `@broken` markers for behavior
  fixed by M9.

Write:

- `docs/roadmaps/2026-04-24-filetest-q32-cleanup/m9-access-lvalue-out-inout/summary.md`

Use this structure:

```markdown
# Summary: M9 Access L-values for `out` / `inout`

## What was built

- ...

## Decisions for future reference

#### <short title>

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Capture only decisions worth future reference. Likely candidates:

- Temp/writeback is the default for scalar/vector/matrix access leaves.
- Direct addresses are used only for stable aggregate storage compatible with
  the existing aggregate pointer ABI.
- Uniform access paths remain rejected before writable call lowering.
- `jit.q32` stays out of M9 acceptance.

If a candidate is already obvious from `00-design.md` and not likely to be
relitigated, omit it.

## Validate

Run formatting:

```bash
cargo +nightly fmt
```

Run focused Rust validation:

```bash
cargo check -p lps-frontend
cargo test -p lps-frontend
```

Run accepted q32 filetests:

```bash
scripts/glsl-filetests.sh --target wasm.q32
scripts/glsl-filetests.sh --target rv32c.q32
scripts/glsl-filetests.sh --target rv32n.q32
```

Run required firmware validation for shader-pipeline changes when practical:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If time allows before final commit/push, run the broader local CI gate:

```bash
just check
just test-filetests
```

If any validation command fails due to known unrelated failures, report the
specific failure and why it is unrelated. Do not hide failures by weakening
tests or adding skip markers.
