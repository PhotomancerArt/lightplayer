# Scope Of Phase

Document the LPIR memory alignment contract for byte, halfword, and word
loads/stores.

Out of scope:

- Do not change backend lowering behavior in this phase.
- Do not add new LPIR ops for unaligned loads.
- Do not implement emulator ISA-profile gating.

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

Update `lp-shader/lpir/src/lpir_op.rs` comments for memory ops so the contract
is explicit:

- `Load8U` / `Load8S`: byte-addressable, no alignment requirement beyond
  addressability.
- `Store8`: byte-addressable.
- `Load16U` / `Load16S`: `base + offset` must be 2-byte aligned.
- `Store16`: `base + offset` must be 2-byte aligned.
- `Load` / 32-bit store op variants: `base + offset` must be 4-byte aligned.

If there is existing LPIR documentation elsewhere under `docs/` or
`lp-shader/lpir/src/`, update that too only if it already describes memory ops.
Do not start a new broad LPIR reference rewrite.

Update `docs/reports/2026-04-27-rv32-load16-issue.md` with a short note that
the chosen direction is to require natural alignment for LPIR narrow loads.

If `lpir::validate` has a natural place to document why static validation cannot
generally prove dynamic pointer alignment, add a brief comment only. Do not add
large static analysis in this phase.

# Validate

Run:

```bash
cargo test -p lpir
```

If only comments/docs changed and `cargo test -p lpir` is unexpectedly blocked
by unrelated workspace issues, report the blocker and the exact error.
