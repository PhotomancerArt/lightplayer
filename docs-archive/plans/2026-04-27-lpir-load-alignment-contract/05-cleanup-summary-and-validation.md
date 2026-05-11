# Scope Of Phase

Clean up the plan, run focused validation, and write the plan summary.

In scope:

- Remove temporary TODOs/debug comments introduced by earlier phases.
- Confirm the final diff does not contain the old Cranelift `Load16U`
  decomposition flag or helper path.
- Add `summary.md` for this plan.
- Run focused validation for the touched crates and texture filetests.

Out of scope:

- Committing changes.
- Implementing emulator ISA-profile gating.
- Making broader Cranelift target-feature changes not required by this plan.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope Of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from the
  phase plan.

# Implementation Details

Search the diff for:

- `riscv_decompose_load16u`
- `decompose_load16u`
- `Load16U` comments that still imply byte-aligned/unaligned behavior.
- Any temporary comments added during the phases.

Write `docs/plans/2026-04-27-lpir-load-alignment-contract/summary.md` with:

```markdown
# Summary

## What was built

- ...

## Decisions for future reference

#### Natural Alignment For LPIR Loads

- **Decision:** `Load16*` requires 2-byte alignment and `Load32` requires 4-byte alignment.
- **Why:** RV32 device code can use direct `lh` / `lhu` / `lw` without expensive unaligned sequences.
- **Rejected alternatives:** Byte-addressable `Load16*` by default (costly on RV32); WASM-style alignment hints (not needed yet).
- **Revisit when:** A real shader feature needs unaligned 16-bit reads.

#### ISA Gating Deferred

- **Decision:** Track emulator `rv32imac` ISA-profile gating separately.
- **Why:** Important for false-success prevention, but broader than the Load16 alignment contract.
- **Rejected alternatives:** Fold ISA gating into this cleanup (would expand scope).
```

Keep the summary terse and update the bullets to reflect the actual diff.

# Validate

Run:

```bash
cargo test -p lpir
cargo test -p lp-riscv-emu
cargo test -p lpvm-cranelift
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
```

If any validation cannot be run locally, record exactly what was not run and
why in the phase report.
