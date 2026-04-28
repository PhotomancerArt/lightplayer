# Scope Of Phase

Clean up the alignment-contract change, update the plan summary, and run final
validation.

Out of scope:

- Do not implement emulator ISA-profile gating.
- Do not add unaligned load support.
- Do not make unrelated backend refactors.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-Agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope Of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations.

# Implementation Details

Search the diff for temporary workarounds and remove them:

- `riscv_decompose_load16u`
- "does not legalize sub-i32"
- "TODO" comments introduced by this plan
- Any debug prints

Update `docs/reports/2026-04-27-rv32-load16-issue.md` if needed so it reflects
the final implemented outcome:

- LPIR narrow loads require natural alignment.
- `lpvm-cranelift` no longer has a special unaligned `Load16U` path.
- Emulator strict-mode alignment tests exist.
- Emulator `rv32imac` ISA-profile gating remains follow-up work.

Create `docs/plans/2026-04-27-lpir-load16-alignment-contract/summary.md` with:

```markdown
# Summary

## What Was Built

- ...

## Decisions For Future Reference

#### Natural alignment for LPIR narrow loads

- **Decision:** `Load16*` / `Store16` require 2-byte alignment; 32-bit memory ops require 4-byte alignment.
- **Why:** The product target is RV32IMAC, where ordinary `lh/lhu/lw` are fastest and simplest when naturally aligned.
- **Rejected alternatives:** Byte-aligned `Load16U` everywhere (adds backend cost now); WASM-style alignment hint (not needed for current texture formats).
- **Revisit when:** A real shader or texture format requires odd-address 16-bit loads.
```

Add at most one more decision if the implementation uncovered a real fork in
the road.

# Validate

Run:

```bash
cargo test -p lpir
cargo test -p lp-riscv-emu
cargo test -p lpvm-cranelift
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
```

If time permits, run:

```bash
just check
```

Report any command that could not be run and why.
