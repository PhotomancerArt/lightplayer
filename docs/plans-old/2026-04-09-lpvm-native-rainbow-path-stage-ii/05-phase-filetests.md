# Phase 5: Filetests and Integration

## Scope of Phase

Run filetests to verify control flow works end-to-end. Fix any issues found.

## Code Organization Reminders

- Test early and often
- Fix issues in the appropriate phase file, not here
- This phase is for validation, not new features

## Implementation Details

### 1. Run if-else filetests

```bash
./scripts/filetests.sh --target rv32lp.q32 "control/if_else/basic.glsl" --concise
```

Expected: Tests should pass once lowering and emission are working.

### 2. Run nested if-else

```bash
./scripts/filetests.sh --target rv32lp.q32 "control/if_else/nested.glsl" --concise
```

### 3. Run chained if-else

```bash
./scripts/filetests.sh --target rv32lp.q32 "control/if_else/chained.glsl" --concise
```

### 4. Check for regressions

```bash
./scripts/filetests.sh --target rv32lp.q32 "scalar/int/" --concise
```

Expected: All previously passing tests still pass.

### 5. Common issues to watch for

- **Label not found**: Check that all labels are defined before use (or backpatching works)
- **Wrong branch offset**: Verify PC-relative calculation (branch target is relative to branch instruction)
- **Off-by-one in ranges**: Check the `lower_range` boundaries (end is exclusive)
- **Skip over Else/End**: Ensure these are marked as processed so we don't lower them twice

### 6. Debugging tips

If a test fails:

1. Run with `--debug` to see CLIF/disassembly
2. Check the lowered VInsts (add debug print in `lower_ops`)
3. Verify label positions and branch offsets in emitted code
4. Use the RV32 disassembler to check the machine code

## Validate

```bash
# Core tests
cargo test -p lpvm-native

# Filetests - control flow
cargo run -p lps-filetests-app -- test --target rv32lp.q32 "control/if_else/"

# Filetests - scalar (regression check)
cargo run -p lps-filetests-app -- test --target rv32lp.q32 "scalar/int/"

# ESP32 build check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

All should pass.
