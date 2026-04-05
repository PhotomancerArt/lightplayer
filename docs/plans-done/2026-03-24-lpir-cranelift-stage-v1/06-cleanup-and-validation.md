## Scope of phase

**Cleanup & validation** (final phase).

- Grep for `TODO`, `FIXME`, `dbg!`, `println!` in the diff; remove or file
  follow-ups.
- Ensure **no warnings** in `lpir-cranelift` for default and feature builds.
- **Rustdoc:** brief module docs on `object_bytes_from_ir` / emulator entry
  points and the **feature flag** name.
- Align **error messages** with `CompilerError` / `CompileError` style used in
  Stage IV.

## Plan cleanup

- Write **`summary.md`** in this plan directory describing what landed vs
  deferred (e.g. multi-return emulator, GLSL-string `emu` entry).
- Move **`docs/plans/2026-03-24-lpir-cranelift-stage-v1/`** to
  **`docs/plans-done/`** after implementation is merged (per repo convention).

## Code organization reminders

- Helpers at bottom of files; tests in `mod tests` at top of test-holding
  modules per `.cursorrules`.

## Validate

```bash
cd /Users/yona/dev/photomancer/lp2025/lps && cargo test -p lpir-cranelift
cd /Users/yona/dev/photomancer/lp2025/lps && cargo test -p lpir-cranelift --features riscv32-emu
cd /Users/yona/dev/photomancer/lp2025/lps && cargo clippy -p lpir-cranelift --all-features -- -D warnings
```

```bash
cargo +nightly fmt
```

## Commit

After validation, one commit (or split docs vs code per team preference) using
Conventional Commits, e.g.:

`feat(lpir-cranelift): add RV32 object and emulator path`

with bullet body listing object emission, link, emu tests, feature flag.
