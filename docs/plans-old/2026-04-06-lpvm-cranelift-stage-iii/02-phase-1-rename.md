# Phase 1: Rename lpir-cranelift → lpvm-cranelift

## Goal

Rename the legacy crate and move it from `lp-shader/legacy/` to `lp-shader/`.
This is a mechanical change to establish the new crate name before adding
trait implementations.

## Steps

1. **Rename directory:**
   - `lp-shader/legacy/lpir-cranelift/` → `lp-shader/lpvm-cranelift/`

2. **Update Cargo.toml:**
   - Change `name = "lpir-cranelift"` → `name = "lpvm-cranelift"`
   - Keep all existing dependencies and features

3. **Update workspace members:**
   - Remove `lp-shader/legacy/lpir-cranelift` from workspace
   - Add `lp-shader/lpvm-cranelift` to workspace

4. **Update consumers:**
   - Find all crates depending on `lpir-cranelift`
   - Update to `lpvm-cranelift`
   - Check `lp-engine`, `lps-filetests`, `fw-esp32`, `fw-emu`

5. **Verify builds:**
   ```bash
   cargo check -p lpvm-cranelift
   cargo check -p lpvm-cranelift --target riscv32imac-unknown-none-elf
   cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
   ```

## Files to Modify

- `Cargo.toml` (workspace members)
- `lp-shader/legacy/lpir-cranelift/Cargo.toml` → `lp-shader/lpvm-cranelift/Cargo.toml`
- All crates with `lpir-cranelift` dependency

## Done When

- Directory renamed and moved
- All references updated
- All builds pass
- No functional changes (just renaming)
