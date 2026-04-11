# Phase 9: Cleanup & Validation

## Scope

Final cleanup, validation, and verification that all goals are met.

## Cleanup

### 1. Grep for temporary code

Search for and remove:
- `todo!()` placeholders
- `unimplemented!()` stubs
- Debug `println!` statements
- Dead code (unused imports, functions, etc.)

```bash
# Search for common temporary markers
grep -r "todo!" lp-shader/lpvm-native-fa/src/
grep -r "unimplemented!" lp-shader/lpvm-native-fa/src/
grep -r "println!" lp-shader/lpvm-native-fa/src/
```

### 2. Check for unused imports

```bash
cargo clippy -p lpvm-native-fa --lib 2>&1 | grep "unused_imports"
```

### 3. Verify no legacy type references

Ensure no references to old types:
- `lpir::VReg` should only be used at lowering boundary (with conversion)
- `SymbolRef` should not appear in VInst
- `Option<u32>` for src_op should not appear in VInst variants

```bash
grep -r "SymbolRef" lp-shader/lpvm-native-fa/src/vinst.rs
# Should only show in comments or legacy conversion code
grep -r "Option<u32>" lp-shader/lpvm-native-fa/src/vinst.rs
# Should not appear in enum variants
```

### 4. Verify size constraints

Add and run size tests:

```bash
cargo test -p lpvm-native-fa --lib -- size
```

Expected:
- `VInst` ≤ 32 bytes (ideally ~20)
- `VReg` = 2 bytes
- `VRegSlice` = 4 bytes
- `SymbolId` = 2 bytes
- `RegSet` = 32 bytes
- `Region` ≤ 16 bytes

## Validation

### 1. Full test suite

```bash
cargo test -p lpvm-native-fa --lib
```

All tests should pass.

### 2. Clippy warnings

```bash
cargo clippy -p lpvm-native-fa --lib -- -D warnings
```

No warnings allowed.

### 3. Format check

```bash
cargo fmt -p lpvm-native-fa -- --check
```

All files properly formatted.

### 4. Check vs original lpvm-native

Ensure the original crate still compiles:

```bash
cargo check -p lpvm-native --lib
cargo test -p lpvm-native --lib
```

This verifies we didn't accidentally modify shared files.

## Success Criteria Verification

| Goal | Verification |
|------|--------------|
| VInst enum size reduced from ~88 bytes to ~20 bytes | `cargo test -- size` shows ≤ 32 bytes |
| Call/Ret use VRegSlice (no Vec heap allocation) | Code inspection shows VRegSlice, not Vec |
| SymbolId replaces SymbolRef | Code inspection shows SymbolId(u16) |
| VReg is u16 | Code inspection shows `VReg(pub u16)` |
| RegSet is [u64; 4] | Code inspection and size test |
| defs()/uses() use callbacks | Code inspection shows `for_each_def` / `for_each_use` |
| for_each_def/use take pool parameter | Function signatures verified |
| LoweredFunction carries vreg_pool | Struct definition verified |
| LoweredModule carries symbols | Struct definition verified |
| RegionTree structure defined | Code inspection and tests |

## Plan Summary

Create `summary.md`:

```markdown
# M3.1: Memory-Optimized Refactoring - Summary

## Completed Work

### Type System
- `VReg(pub u16)` — compact virtual register (2 bytes)
- `VRegSlice` — slice into pool for Call/Ret operands (4 bytes)
- `SymbolId(u16)` — interned symbol reference (2 bytes)
- `RegSet([u64; 4])` — fixed-size bitset for liveness (32 bytes)

### VInst Enum
- Shrank from ~88 bytes to ~24 bytes per instruction
- Eliminated heap allocations in Call/Ret
- Replaced `Option<u32>` src_op with `u16` sentinel

### API Changes
- `defs()`/`uses()` → `for_each_def()`/`for_each_use()` (zero allocation)
- Added `pool: &[VReg]` parameter for slice resolution

### Lowering Infrastructure
- `ModuleSymbols` — module-level symbol interning
- `LoweredFunction` — carries vreg_pool
- `LoweredModule` — top-level container with symbols
- `RegionTree` — arena-based region tree (for M4)

### Memory Savings
| Structure | Before | After | Savings |
|-----------|--------|-------|---------|
| VInst (100 instrs) | ~8.8 KB | ~2.4 KB | ~73% |
| Call heap allocs | 3 per call | 0 | 100% |
| RegSet | ~2 KB+ | 32 bytes | ~98% |

## Next Steps (M4)

- Build region tree during lowering
- Implement recursive liveness on region tree
- Build backward walk allocator shell
```

## Final Commands

```bash
# Full validation
cargo test -p lpvm-native-fa --lib
cargo clippy -p lpvm-native-fa --lib -- -D warnings
cargo fmt -p lpvm-native-fa

# Verify original crate still works
cargo test -p lpvm-native --lib
```

Ready to move to `docs/plans-done/` and commit.
