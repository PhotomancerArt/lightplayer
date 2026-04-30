# Phase 6: Filetest Validation + Cleanup

## Scope

Run all filetests under `rv32fa`, fix remaining bugs, clean up warnings
and temporary code.

## Code Organization Reminders

- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Remove all TODO comments added in earlier phases.

## Implementation Details

### 1. Run filetests

```bash
# Run all rv32fa filetests
cargo run -p lps-filetests-app -- test --target rv32fa.q32

# Or specific categories:
cargo run -p lps-filetests-app -- test --target rv32fa.q32 scalar/
cargo run -p lps-filetests-app -- test --target rv32fa.q32 operators/
cargo run -p lps-filetests-app -- test --target rv32fa.q32 control-flow/
```

Categorize failures:
- **Wrong result**: allocator correctness bug → fix
- **Compilation error**: missing VInst/PInst handling → fix
- **Linking error**: symbol not found → likely builtin table issue (not M3 scope)
- **Timeout/hang**: infinite loop in allocation → fix

### 2. Fix bugs found in filetests

Expected areas of bugs:
- Branch fixup offset computation (off-by-one, sign errors)
- Call clobber set not including all necessary registers
- IfThenElse reconciliation emitting moves in wrong order
- Loop back-edge fixups creating register conflicts (parallel move needed)
- Param precoloring for multi-param functions
- Sret buffer offset miscalculation

### 3. Clean up warnings

```bash
cargo check -p lpvm-native 2>&1 | grep warning
cargo check -p lp-cli 2>&1 | grep warning
```

Fix all warnings: unused variables, unused imports, dead code.

### 4. Remove temporary code

Grep for TODO comments added during M3:

```bash
git diff --name-only | xargs grep -n 'TODO'
```

Remove or resolve all TODOs.

### 5. Remove stale error variants

Ensure these are gone:
- `AllocError::UnsupportedControlFlow` (removed in phase 4)
- `AllocError::UnsupportedCall` (removed in phase 2)
- `AllocError::UnsupportedSelect` (removed in phase 1)
- `AllocError::UnsupportedSret` (removed in phase 5)

Only `AllocError::TooManyArgs` should remain (if kept).

### 6. Final validation

```bash
# All lpvm-native tests pass
cargo test -p lpvm-native

# Filetest target tests pass
cargo test -p lps-filetests

# CLI pipeline works
cargo check -p lp-cli

# Filetests pass
cargo run -p lps-filetests-app -- test --target rv32fa.q32

# Host check
cargo check -p lpa-server
```

### 7. Plan cleanup

Add a summary of the completed work to `docs/plans/2026-04-11-fastalloc3-m3/summary.md`.

Move plan files to `docs/plans-done/`.

### 8. Commit

```
feat(native-fa): implement control flow, calls, and sret in fa_alloc (M3)

- Add label fixup system to Rv32Emitter for branch resolution
- Handle BrIf, Br, Label, Select32 VInsts in backward walk
- Implement Call with caller-saved spill/reload and ABI arg/ret placement
- Implement IfThenElse region walking with register state reconciliation
- Implement Loop region walking with back-edge fixup moves
- Handle sret calls (>2 return scalars via stack buffer)
- Wire FuncAbi for param precoloring and call clobber sets
- All rv32fa filetests pass
```
