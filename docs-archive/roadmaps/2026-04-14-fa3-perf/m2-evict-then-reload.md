# Milestone 2: Evict-then-Reload Call Clobber

## Goal

Replace the save/restore pair approach for caller-saved registers at calls
with regalloc2-style evict-then-reload. Eliminates the ordering hazard
documented in `docs/design/native/fa-impl-notes.md` and simplifies the
arg-eviction fixup logic.

## Suggested plan name

`fa3-perf-m2`

## Scope

**In scope**:
- Refactor step 3 of `process_call` in `fa_alloc/walk.rs`:
  - Instead of Before(call) save + After(call) restore, evict the vreg from
    the pool and emit only After(call) reload.
  - The eviction forces the vreg's def (reached later in the backward walk)
    to write directly to its spill slot.
- Remove `before_saves` vector and the `before_saves.retain()` fixup for
  arg evictions.
- Simplify the callee-saved eviction fixup (only callee-saved evictions need
  explicit restores now).
- Update/add call filetests as needed.

**Out of scope**:
- Pool LRU changes (M1, should be done first).
- Emit-layer changes (M3).
- Lowering changes (M4-M5).

## Key decisions

- The backward-walk equivalent of regalloc2's eviction: at a call, remove
  clobbered-reg occupants from the pool and emit only a post-call reload
  (After: slot -> reg). No save needed — the eviction forces the def to write
  to the spill slot. This is described in `fa-impl-notes.md`.

- The `before_saves` vector becomes unnecessary. The edit push order simplifies
  to: After(restores), After(ret_moves), Before(arg_moves).

## Deliverables

- Refactored `process_call` in `fa_alloc/walk.rs`.
- Passing call filetests (`filetests/call/`).
- Passing unit tests in `fa_alloc/walk.rs` and `fa_alloc/mod.rs`.

## Dependencies

M1 (pool LRU reuse) should be completed first so that register assignments
are stable before changing the call clobber strategy.

## Estimated scope

~30-50 lines changed in `walk.rs`. The change is structurally simple but
requires careful verification against all call test cases.
