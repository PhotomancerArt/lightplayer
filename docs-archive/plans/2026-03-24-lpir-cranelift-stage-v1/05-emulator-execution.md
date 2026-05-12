## Scope of phase

Add **`emu_run.rs`**: given **`ElfLoadInfo`** (from phase 04), configure
`lp_riscv_emu::Riscv32Emulator`, resolve a **function symbol** by name, run with
reasonable **`EmulatorOptions`** (stack/memory/instruction limit), and read back
results.

Expose a small public API for tests, e.g.:

- `run_linked_q32_i32(ir: &IrModule, func_name: &str, args: &[i32]) -> Result<i32, …>`
  built from `object_bytes_from_ir` → link → emulate,

or lower-level pieces so tests compose.

## Code organization reminders

- Mirror **calling convention** assumptions from old `GlslEmulatorModule` / tests
  for scalar Q32 (registers vs stack). Start with the simplest case that matches
  existing RISC-V 32 Cranelift output (e.g. single `i32` return).
- Prefer **one** high-level helper for tests plus thin primitives.

## Implementation details

- Reuse **trap/timeout** handling patterns from `lps-cranelift` where
  applicable; do not copy large slabs of `GlslEmulatorModule` if a minimal path
  suffices for V1.
- **Multi-return / struct return:** defer to follow-up if blocking; document in
  `00-notes.md` under Notes if punted.

## Tests

- **End-to-end** (feature + builtins ELF available): LPIR equivalent of a trivial
  Q32 shader (`fadd` two constants or identity) → object → link → emulator →
  assert Q32-encoded result.
- If builtins unavailable in CI, gate with `#[ignore]` and run locally.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lps && cargo test -p lpvm-cranelift --features riscv32-emu
```

`cargo +nightly fmt`.
