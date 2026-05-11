# Scope Of Phase

Document the LPIR scalar load alignment contract in the source of truth and in
the RV32 load issue report.

In scope:

- Update `lp-shader/lpir/src/lpir_op.rs` comments for `Load8U`, `Load8S`,
  `Load16U`, and `Load16S`.
- If nearby 32-bit load/store comments exist, clarify that 32-bit memory
  accesses require 4-byte alignment.
- Update `docs/reports/2026-04-27-rv32-load16-issue.md` to say the chosen
  contract is natural alignment, not an unresolved question.

Out of scope:

- Changing runtime behavior.
- Adding a new unaligned load opcode.
- Changing WASM or interpreter semantics.

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

Update `lp-shader/lpir/src/lpir_op.rs` so the load comments are explicit:

```rust
/// 8-bit zero-extending load: `dst = u8[base + offset]`.
/// No alignment requirement.
Load8U { ... }

/// 16-bit zero-extending load.
/// Precondition: `base + offset` is 2-byte aligned.
Load16U { ... }
```

Use the same wording for `Load8S` and `Load16S`.

If `Load` / `Store` comments nearby describe 32-bit values, clarify:

```rust
/// Precondition: `base + offset` is 4-byte aligned.
```

Update `docs/reports/2026-04-27-rv32-load16-issue.md`:

- Replace language that describes LPIR alignment as unresolved with the chosen
  contract.
- Keep the note that WASM/interp are permissive internally, but generated LPIR
  should respect the contract.
- Keep the recommendation to avoid adding byte-aligned Load16 behavior unless
  a future use case requires a distinct operation.

# Validate

This phase is primarily docs/comments. Run:

```bash
cargo test -p lpir
```
