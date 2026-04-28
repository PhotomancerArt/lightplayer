# Scope Of Phase

Add focused emulator tests proving strict-mode RV32 load alignment behavior.

In scope:

- Add tests for aligned and misaligned `lh`, `lhu`, and `lw` behavior in
  `lp-riscv-emu`.
- Confirm strict mode is the default: `allow_unaligned_access` should remain
  false unless explicitly enabled.
- Prefer testing through real decoded instructions rather than only calling
  `Memory` helper methods.

Out of scope:

- Implementing ISA-profile gating for `rv32imac`.
- Changing emulator default alignment behavior.
- Changing LPVM filetest harness behavior unless it is unexpectedly enabling
  unaligned access for RV32 paths.

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

Use existing emulator test style. Relevant files:

- `lp-riscv/lp-riscv-emu/src/emu/memory.rs`
- `lp-riscv/lp-riscv-emu/src/emu/executor/load_store.rs`
- `lp-riscv/lp-riscv-inst/src/encode.rs`
- Existing unaligned test:
  `lp-riscv/lp-riscv-inst/tests/instruction_tests.rs`

Add tests in the most local appropriate module, preferably near existing
load/store executor tests if helpers are available.

Test cases:

1. `lhu` at an even address succeeds.
2. `lhu` at an odd address returns `EmulatorError::UnalignedAccess`.
3. `lh` at an even address succeeds.
4. `lh` at an odd address returns `EmulatorError::UnalignedAccess`.
5. `lw` at a 4-byte-aligned address succeeds.
6. `lw` at a 2-byte-only-aligned address returns `EmulatorError::UnalignedAccess`.

Prefer direct instruction execution through `Riscv32Emulator::step()` using
encoded instructions. This ensures the decoder/executor path, not just the
memory helper, enforces the behavior.

If using encoded instruction bytes, keep helper functions small and at the
bottom of the test module.

# Validate

Run:

```bash
cargo test -p lp-riscv-emu
```

If tests are added outside that crate, also run the matching crate's test
command.
