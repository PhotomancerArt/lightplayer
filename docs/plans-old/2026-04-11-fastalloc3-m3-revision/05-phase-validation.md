# Phase 5: Filetest Validation + Cleanup

## Scope of phase

Run all filetests under `rv32fa`, fix failures, clean up temporary code.

## Code Organization Reminders

- Any temporary code should have a TODO comment.
- Remove TODOs that are resolved.
- Keep code compiling and tests passing.

## Implementation Details

### 1. Run Filetests

```bash
cargo test -p lps-filetests -- rv32fa
```

Or specifically:
```bash
cargo test -p lps-filetests -- test_file_lpvm_native
```

### 2. Fix Failures

For each failing test:
1. Understand the failure (wrong output, crash, error)
2. Fix the allocator/walker/emitter
3. Re-run to verify

Common issues to check:
- Spill slot offset calculation (should be `-(slot+1)*4` from fp)
- Boundary spill/reload ordering
- Label fixup resolution
- Branch displacement encoding

### 3. Cleanup

Search for and remove/finalize:
- `TODO` comments
- `unimplemented!()` or `todo!()`
- Debug prints (`println!`, `dbg!`)
- Unused imports or dead code
- Temporary error variants (like `UnsupportedControlFlow` once IfThenElse/Loop work)

### 4. Final Validation

```bash
# All lpvm-native tests pass
cargo test -p lpvm-native

# Filetests pass
cargo test -p lps-filetests

# Firmware check passes
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Format
cargo +nightly fmt -p lpvm-native

# No warnings
cargo check -p lpvm-native 2>&1 | grep -i warning || true
```

## Deliverables

- All filetests pass under `rv32fa`
- No TODOs remaining (or documented why they're kept)
- Clean build with no warnings
- Formatted code
