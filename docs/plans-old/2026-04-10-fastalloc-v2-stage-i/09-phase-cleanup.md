# Phase 9: Cleanup

## Scope

Remove old fastalloc code and rename `rv32fa` to `rv32`.

## Steps

### 1. Remove old fastalloc from rv32/

Delete:
- `lp-shader/lpvm-native/src/isa/rv32/fastalloc.rs`
- References to fastalloc in `lp-shader/lpvm-native/src/isa/rv32/mod.rs`

### 2. Remove old allocation structures

Delete from `lp-shader/lpvm-native/src/alloc.rs`:
- `FastAllocation` struct
- `FastEdit` struct
- `OperandHome` variants for fastalloc

Keep `Allocation` (linear scan uses it).

### 3. Rename rv32fa to rv32

After filetests pass:

```bash
mv lp-shader/lpvm-native/src/isa/rv32 lp-shader/lpvm-native/src/isa/rv32_old
mv lp-shader/lpvm-native/src/isa/rv32fa lp-shader/lpvm-native/src/isa/rv32
```

Update all `use` statements and module declarations.

### 4. Update config.rs

Remove `RegAllocAlgorithm::Fast` option if we're making it the default.

Or keep it for comparison testing.

### 5. Final validation

```bash
# Host tests
cargo test -p lpvm-native --lib
cargo test -p lp-cli -- shader_rv32

# Firmware check (critical)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf

# Scene render tests
cargo test -p fw-tests --test scene_render_emu
```

## Notes

- Don't delete old code until new pipeline passes filetests
- Keep backup branch: `feature/fastalloc-v2-complete`
- Document breaking changes in commit message
