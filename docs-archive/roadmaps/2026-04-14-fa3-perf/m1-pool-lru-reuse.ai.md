# Milestone 1: Pool LRU Register Reuse

## Goal

Reduce callee-saved register usage by making `pool.free()` recycle registers
to the front of the LRU, so freed t-regs are reused before untouched s-regs.

## Suggested plan name

`fa3-perf-m1`

## Scope

**In scope**:
- Change `RegPool::free()` in `fa_alloc/pool.rs` to move the freed register
  to the front (index 0) of the LRU list.
- Update snapshot tests that assert specific register assignments.
- Verify correctness via existing call filetests and unit tests.
- Measure improvement on `caller-save-pressure.glsl`.

**Out of scope**:
- Call clobber refactor (M2).
- Emit/prologue changes (M3).
- Any new VInst types or lowering changes.

## Key decisions

- "Front of LRU" means position 0 (least recently used). This makes the freed
  register the first candidate for the next `alloc()` call, which prefers free
  registers over eviction. When subsequently allocated, it moves to the back
  (MRU), so eviction ordering is unaffected.

- This is safe because free registers have no occupant and cannot be eviction
  victims. The only effect is preferring recently-freed registers over
  never-used ones.

## Deliverables

- Modified `RegPool::free()` in `fa_alloc/pool.rs`.
- Updated snapshot tests in `fa_alloc/mod.rs`.
- Before/after instruction counts on the perf suite.

## Dependencies

None. This is the first milestone.

## Estimated scope

~5 lines of code change in `pool.rs`, plus snapshot test updates.
