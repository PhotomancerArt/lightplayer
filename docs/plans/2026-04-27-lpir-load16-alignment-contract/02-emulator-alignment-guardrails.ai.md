# Scope Of Phase

Add focused tests proving the RV32 emulator's strict mode traps misaligned
halfword and word loads/stores.

Out of scope:

- Do not add ISA-profile gating in this phase.
- Do not change emulator default alignment behavior unless tests reveal it is
  already wrong.
- Do not change LPVM backend code.

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

Inspect:

- `lp-riscv/lp-riscv-emu/src/emu/memory.rs`
- `lp-riscv/lp-riscv-emu/src/emu/executor/load_store.rs`
- Existing tests in `lp-riscv/lp-riscv-inst/tests/instruction_tests.rs`
  and `lp-riscv/lp-riscv-emu/src/emu/executor/load_store.rs`.

Add focused tests in the most local appropriate test module. The tests should
cover strict-mode behavior:

- `lhu` at an even address succeeds.
- `lhu` at an odd address returns an unaligned access error.
- `lh` at an odd address returns an unaligned access error.
- `lw` at an address not divisible by 4 returns an unaligned access error.

If there are already direct `Memory::read_halfword` / `read_word` tests, add
coverage there. If executor-level tests are easier and clearer, encode small
instruction sequences and step the emulator.

Also verify filetest RV32 paths instantiate `Riscv32Emulator` without calling
`with_allow_unaligned_access(true)`. If that is already true, do not change
code; mention it in the report-back.

# Validate

Run:

```bash
cargo test -p lp-riscv-emu
```

If the crate name differs or targeted tests are faster, run the smallest
equivalent command and report it.
