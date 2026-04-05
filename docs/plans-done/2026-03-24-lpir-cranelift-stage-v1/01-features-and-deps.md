## Scope of phase

Add Cargo **feature** `riscv32-emu` (name locked for this plan) and **optional dependencies** for object emission and RISC-V:
`cranelift-object`, `object`, `lp-riscv-elf`, `lp-riscv-emu`, `lp-riscv-inst` as
needed.

Enable **`riscv32`** on `cranelift-codegen` for that feature (alongside existing
`host-arch` for default JIT).

Gate **all** new code behind `#[cfg(feature = "riscv32-emu")]` so default
`cargo test -p lpir-cranelift` does not require builtins artifacts or emulator.

## Code organization reminders

- Prefer granular files; keep feature-specific code in dedicated modules.
- Document the feature in crate-level rustdoc on `lib.rs`.

## Implementation details

- Mirror dependency versions/features from `lps-cranelift/Cargo.toml`
  (`emulator` feature block) but trim anything AST-specific.
- Default features for `lpir-cranelift` should remain usable for host JIT only.
- If `build.rs` is required for later phases, add a **stub** `build.rs` that
  does nothing when the feature is off, or use `required-features` on tests —
  document the chosen approach.

## Tests

- No behavioral tests yet; confirm `cargo check -p lpir-cranelift` and
  `cargo check -p lpir-cranelift --features riscv32-emu` both succeed (once
  follow-on phases fill in modules, or use empty cfg modules).

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lps && cargo check -p lpir-cranelift
cd /Users/yona/dev/photomancer/lp2025/lps && cargo check -p lpir-cranelift --features riscv32-emu
```

Run `cargo +nightly fmt` on touched files.
