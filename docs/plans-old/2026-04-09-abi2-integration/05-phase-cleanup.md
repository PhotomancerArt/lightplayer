## Scope of Phase

Clean up temporary code, fix warnings, format, and validate all tests pass.

## Code Organization Reminders

- Grep for TODO, FIXME, XXX comments
- Remove any debug prints
- Check for unused imports
- Run clippy if available
- Format with nightly

## Cleanup Checklist

### 1. Grep for TODOs

```bash
grep -rn "TODO\|FIXME\|XXX" lp-shader/lpvm-native/src/abi/
grep -rn "TODO\|FIXME\|XXX" lp-shader/lpvm-native/src/regalloc/
grep -rn "TODO\|FIXME\|XXX" lp-shader/lpvm-native/src/isa/rv32/emit.rs
grep -rn "TODO\|FIXME\|XXX" lp-shader/lpvm-native/src/rt_emu/
```

Expected TODOs to resolve:
- Any `TODO(abi2)` markers should be done
- `TODO(future)` can stay with issue references

Remove debug prints:
```bash
grep -rn "println!\|eprintln!\|dbg!" lp-shader/lpvm-native/src/abi/
grep -rn "println!\|eprintln!\|dbg!" lp-shader/lpvm-native/src/regalloc/
grep -rn "println!\|eprintln!\|dbg!" lp-shader/lpvm-native/src/isa/rv32/emit.rs
grep -rn "println!\|eprintln!\|dbg!" lp-shader/lpvm-native/src/rt_emu/
```

### 2. Fix warnings

```bash
cargo check -p lpvm-native 2>&1 | grep -i warning
cargo check -p lpvm-native --tests 2>&1 | grep -i warning
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server 2>&1 | grep -i warning
```

Fix all warnings in:
- `abi2/` module
- `regalloc/` changes
- `isa/rv32/emit.rs` changes
- `rt_emu/` changes

### 3. Format

```bash
cargo +nightly fmt -p lpvm-native
```

Verify no formatting issues:
```bash
cargo +nightly fmt -p lpvm-native -- --check
```

### 4. Run all tests

```bash
cargo test -p lpvm-native 2>&1 | tail -20
```

Expected: 90+ tests passing (82 original + 8-10 new from this plan)

### 5. Filetest validation

```bash
# Run sret-related filetests
cargo test -p lps-filetests -- spill_pressure 2>&1
cargo test -p lps-filetests -- mat4 2>&1
```

Expected: All mat4 and spill_pressure tests pass.

### 6. Embedded build check

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

Should complete without errors or warnings.

## Validation Commands Summary

```bash
#!/bin/bash
set -e

echo "=== Check ==="
cargo check -p lpvm-native
cargo check -p lpvm-native --tests

echo "=== Format ==="
cargo +nightly fmt -p lpvm-native -- --check

echo "=== Tests ==="
cargo test -p lpvm-native

echo "=== Filetests ==="
cargo test -p lps-filetests -- spill_pressure
cargo test -p lps-filetests -- mat4

echo "=== Embedded ==="
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

echo "=== All checks passed ==="
```

## Post-Cleanup Tasks

### 1. Create summary.md

Write final summary documenting:
- What was implemented
- Test results
- Any known limitations

### 2. Move plan to done

```bash
mv docs/plans/2026-04-09-abi-integration docs/plans-done/
```

### 3. Commit

```bash
git add -A
git commit -m "$(cat <<'EOF'
feat(lpvm-native): integrate abi into compiler pipeline

Wire abi module through regalloc, emitter, and runtime:
- Add FuncAbi helpers: precolor_of, sret_word_count, stack_alignment
- Regalloc: respect precolors, allocatable set, s1 reservation for sret
- Emitter: prologue with frame layout, sret stores in VInst::Ret
- Runtime: sret buffer allocation, arg shifting, result readback

Enables sret calling convention for mat4 and large returns.
All 90+ tests pass, spill_pressure.glsl and mat4 tests working.
EOF
)"
```

## Success Criteria

- [ ] No TODOs remaining in new code
- [ ] No warnings in `cargo check`
- [ ] All 90+ tests pass
- [ ] `spill_pressure.glsl` passes
- [ ] All mat4 tests pass
- [ ] `fw-esp32` builds cleanly
- [ ] Code formatted
- [ ] Plan moved to `docs/plans-done/`
- [ ] Changes committed
