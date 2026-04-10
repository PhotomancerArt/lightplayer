## Phase 5: Cleanup and Documentation

### Scope

Clean up temporary code, add documentation, final validation. This is the
cleanup phase of the plan.

### Code Organization Reminders

- Remove debug prints and temporary code
- Add doc comments to public types and functions
- Fix any compiler warnings
- Keep the code tidy

### Cleanup Tasks

**1. Remove debug prints**

Grep for and remove any temporary debug code:

```bash
grep -r "eprintln!" lp-shader/lpvm-native/src/regalloc/
grep -r "eprintln!" lp-shader/lpvm-native/src/isa/rv32/emit.rs
```

**2. Add doc comments**

In `regalloc/mod.rs`:
```rust
/// New allocation output format for per-instruction operand assignments
/// and explicit move edits.
///
/// This replaces the static [`Allocation`] format which used a single
/// function-wide `vreg_to_phys` map. The new format enables:
/// - Per-instruction register assignments
/// - Values that can be evicted to stack and reloaded into different registers
/// - Explicit move edits that the emitter splices into the instruction stream
pub struct FastAllocation {
    // ... fields with doc comments ...
}
```

In `regalloc/adapter.rs`:
```rust
/// Adapter that converts [`Allocation`] (static vreg→phys map) to
/// [`FastAllocation`] (per-instruction assignments + edit list).
///
/// This is used in M1 to prove the new format works without changing the
/// allocation algorithm. The adapter replicates the old emitter's call-save
/// logic as explicit edits.
pub struct AllocationAdapter;
```

**3. Fix compiler warnings**

```bash
cargo check -p lpvm-native --features emu 2>&1 | grep warning
cargo clippy -p lpvm-native --features emu 2>&1 | grep warning
```

**4. Add module-level documentation**

In `regalloc/adapter.rs` top:
```rust
//! Allocation adapter: converts static Allocation to FastAllocation.
//!
//! This module provides [`AllocationAdapter`] which bridges the old
//! allocation output format ([`Allocation`]) to the new format
//! ([`FastAllocation`]). This allows incremental migration: the existing
//! allocators (greedy, linear scan) produce the old format, and the adapter
//! converts it to the new format for consumption by the new emitter path.
//!
//! In future milestones, the fastalloc allocator will produce FastAllocation
//! directly, and this adapter can be removed.
```

**5. Update DESIGN.md with any changes**

If implementation diverged from the design (e.g., different approach for
IConst32), document the actual approach.

### Documentation Updates

**Update `docs/design/native/2026-04-09-fastalloc-mini.md`:**

Add a note at the top:
```markdown
## Implementation Status

- [x] M1: New allocation output format (April 2026)
  - FastAllocation types defined
  - AllocationAdapter implemented
  - New emitter path with edit splicing
  - All filetests pass

- [ ] M2: Backward-walk allocator (not started)
- [ ] M3: Control flow support (not started)
- [ ] M4: Perf validation and cleanup (not started)
```

**Update `docs/design/lpvm/notes.md`:**

Add a section documenting the performance context that led to fastalloc:
```markdown
## 2026-04-10: Fastalloc M1 Complete

First milestone of fastalloc register allocator complete. New allocation
output format (`FastAllocation`) with per-instruction operand assignments
and explicit move edits. Old allocators (greedy, linear scan) adapted via
`AllocationAdapter`. New emitter path in place, all filetests pass.

Next: M2 backward-walk allocator.
```

### Final Validation

**1. All filetests pass**

```bash
scripts/glsl-filetests.sh --target rv32lp lpvm/native
```

**2. ESP32 build works**

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server
```

**3. Emulator tests compile**

```bash
cargo test -p fw-tests --test scene_render_emu --no-run
```

**4. No warnings**

```bash
cargo check -p lpvm-native 2>&1 | grep -c warning || echo "0 warnings"
```

### Summary Document

Create `docs/plans/2026-04-10-fastalloc-stage-i/summary.md`:

```markdown
# M1: New Allocation Output Format — Summary

## Completed Work

- Defined `FastAllocation` types with per-instruction operand assignments
- Implemented `AllocationAdapter` to convert old Allocation format
- Implemented new emitter path with edit splicing
- All filetests pass with new path

## Key Design Decisions

1. Flat operand array for cache-friendly access
2. High-level Move edits lowered by emitter
3. Adapter pattern for incremental migration
4. Dual emitter paths for validation

## Files Changed

- `lp-shader/lpvm-native/src/regalloc/mod.rs`: FastAllocation types
- `lp-shader/lpvm-native/src/regalloc/adapter.rs`: NEW
- `lp-shader/lpvm-native/src/isa/rv32/emit.rs`: new emitter path
- `lp-shader/lpvm-native/src/config.rs`: USE_FAST_ALLOC_EMIT flag

## Next Steps

M2: Implement backward-walk allocator in `regalloc/fastalloc.rs`.
```

### Plan Completion

After summary is written, move plan files to done:

```bash
mkdir -p docs/plans-done/2026-04-10-fastalloc-stage-i
mv docs/plans/2026-04-10-fastalloc-stage-i/* docs/plans-done/2026-04-10-fastalloc-stage-i/
rmdir docs/plans/2026-04-10-fastalloc-stage-i
```
