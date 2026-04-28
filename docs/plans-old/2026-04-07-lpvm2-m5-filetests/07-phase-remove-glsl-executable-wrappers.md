# Phase 7: Remove `GlslExecutable` wrappers from filetests

## Scope of phase

Delete or reduce to zero:

- `lpir_jit_executable.rs`
- `lpir_rv32_executable.rs`
- `wasm_runner.rs` / `wasm_link.rs` (if fully replaced by `lpvm-wasm`)

**`q32_exec_common.rs`:** Shrink to helpers only (signature maps, `LpsValue` ↔ flat `i32`, comparison). Remove **`Q32ShaderExecutable`** / **`GlslExecutable`** impls if obsolete.

**`Cargo.toml`:** Drop `lps-exec` dependency from `lps-filetests` if no longer used.

Update **`mod.rs`** exports.

## Code Organization Reminders

- Delete dead code; grep for `GlslExecutable` / `lps_exec` under `lps-filetests`.

## Implementation Details

- Ensure **no** behavior change vs pre-phase-7 for the same filetests corpus.

## Validate

```bash
cargo check -p lps-filetests
rg "lps_exec|GlslExecutable" lp-shader/lps-filetests || true
just test-filetests
# or: ./scripts/filetests.sh
```

Per AGENTS.md / `.cursorrules` if shader stack touched:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
