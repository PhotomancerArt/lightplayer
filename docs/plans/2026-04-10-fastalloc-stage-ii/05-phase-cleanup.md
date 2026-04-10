## Phase 5: Cleanup and Documentation

### Scope

Clean up temporary code, add documentation, update design docs, and commit
the M2 work.

### Cleanup Tasks

**1. Remove debug prints**

Check for any `eprintln!` or debug code in `fastalloc.rs` and remove.

**2. Add doc comments**

In `fastalloc.rs`:

```rust
//! Fast backward-walk register allocator for straight-line code.
//!
//! This allocator processes VInsts in reverse order, tracking liveness
//! and allocating registers on demand. Values can be evicted to spill slots
//! when register pressure is high, then reloaded when needed.
//!
//! Key features:
//! - Single-pass backward walk (no interval building)
//! - Lazy spill slot allocation
//! - IConst32 rematerialization (no spill needed)
//! - LRU eviction heuristic
//!
//! Limitations (to be addressed in M3):
//! - Straight-line code only (no control flow)
//! - No live range splitting (spilled values reloaded before each use)

/// Fast allocator for straight-line VInst sequences.
///
/// The allocator performs a backward walk over instructions, maintaining
/// a set of live values and their current homes (register or spill slot).
pub struct FastAllocator;
```

**3. Fix compiler warnings**

```bash
cargo check -p lpvm-native 2>&1 | grep warning
cargo clippy -p lpvm-native 2>&1 | grep warning
```

**4. Format code**

```bash
cargo +nightly fmt -p lpvm-native
```

**5. Update design docs**

Update `docs/design/native/2026-04-09-fastalloc-mini.md`:

```markdown
## Implementation Status

- [x] M1: New allocation output format (April 2026)
- [x] M2: Backward-walk allocator for straight-line code (April 2026)
  - Core algorithm implemented
  - LRU eviction
  - IConst32 rematerialization
  - Straight-line tests passing
  - Control flow tests fail (expected, needs M3)

- [ ] M3: Control flow support (blocks, branches, loops)
- [ ] M4: Perf validation and cleanup
```

**6. Update roadmap notes**

Update `docs/design/lpvm/notes.md` with M2 summary.

### Summary Document

Create `docs/plans/2026-04-10-fastalloc-stage-ii/summary.md`:

```markdown
# M2: Backward-Walk Allocator — Summary

## Completed Work

- Implemented backward-walk register allocator in `regalloc/fastalloc.rs`
- Core algorithm: defs free registers, uses allocate/reload, call clobbers evict
- LRU eviction heuristic for spill victim selection
- IConst32 rematerialization (no spill slots needed)
- Integration with existing emitter via `USE_FASTALLOC` flag

## Validation Results

### Passing (straight-line code)
- native-call-simple.glsl, native-call-multi-args.glsl, etc.
- All perf tests without control flow

### Failing (control flow - expected)
- native-call-control-flow.glsl (has BrIf)
- Will be addressed in M3

## Files Added/Modified

- `lp-shader/lpvm-native/src/regalloc/fastalloc.rs` - NEW
- `lp-shader/lpvm-native/src/regalloc/mod.rs` - export FastAllocator
- `lp-shader/lpvm-native/src/config.rs` - USE_FASTALLOC flag
- `lp-shader/lpvm-native/src/isa/rv32/emit.rs` - integrate fastalloc path

## Next Steps

M3: Add control flow support (block splitting, boundary liveness reconciliation).
```

### Commit

Create a commit following conventional commits format:

```bash
git add lp-shader/lpvm-native/src/regalloc/fastalloc.rs \
       lp-shader/lpvm-native/src/regalloc/mod.rs \
       lp-shader/lpvm-native/src/config.rs \
       lp-shader/lpvm-native/src/isa/rv32/emit.rs

git commit -m "$(cat <<'EOF'
feat(lpvm-native): implement backward-walk register allocator (M2)

- Add FastAllocator with backward-walk algorithm for straight-line code
- Implement LRU eviction and lazy spill slot allocation
- Support IConst32 rematerialization (no spill slots)
- Handle call clobbers by evicting live caller-saved registers
- Add USE_FASTALLOC config flag for selection
- Integrate fastalloc path into emitter
- All straight-line filetests pass; control flow returns error (M3)
EOF
)"
```

### Plan Completion

After commit, move plan files:

```bash
mkdir -p docs/plans-done/2026-04-10-fastalloc-stage-ii
mv docs/plans/2026-04-10-fastalloc-stage-ii/* docs/plans-done/2026-04-10-fastalloc-stage-ii/
rmdir docs/plans/2026-04-10-fastalloc-stage-ii
```

### Final Verification

```bash
# Check clean git status
git status

# Verify all tests pass (with fastalloc disabled or on straight-line tests)
cargo test -p lpvm-native --lib
scripts/glsl-filetests.sh --target rv32lp lpvm/native

# Verify ESP32 build
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server
```
