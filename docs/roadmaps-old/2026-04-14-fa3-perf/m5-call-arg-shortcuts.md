# Milestone 5: Call Arg/Ret Register Shortcuts

## Goal

Eliminate unnecessary register shuffling for call arguments that are already
in (or could be placed directly into) the correct ABI register. Currently the
backward walk allocates entry parameters and single-use constants to pool
callee-saved regs, then moves them to arg regs before the call.

## Suggested plan name

`fa3-perf-m5`

## Scope

**In scope**:
- Detect at call processing time when an arg vreg is the function's entry
  parameter and will map to the same arg register (e.g. v0 is param 0 and
  call arg 0 — both use a0). Skip the pool allocation and emit no move.
- Detect single-use constants that are only consumed as call arguments.
  Materialize directly into the arg register rather than into a pool reg.
- Handle the backward-walk implications: the vreg must not be assigned a
  pool register if it will be short-circuited.

**Out of scope**:
- General "rematerialization" of constants (that's a much larger framework).
- Changes to the call ABI itself.

## Key decisions

- This requires the allocator to be partially aware of ABI constraints before
  it encounters the call in the backward walk. One approach: when processing
  a call's arg uses, check if the vreg has no pool home and no spill slot
  (first encounter), and if so, record that its def should target the arg reg
  directly rather than a pool reg.

- Alternatively, a post-allocation peephole could detect and eliminate
  `move pool_reg -> arg_reg` / `entry_move arg_reg -> pool_reg` pairs. This
  is less invasive but only catches the pattern after the fact.

## Deliverables

- Modified call arg handling in `process_call` or a post-alloc optimization
  pass.
- Reduced entry_move + arg_move pairs in the output.
- Updated filetests.

## Dependencies

M1-M3 should be completed first. M4 (immediate folding) is independent and
could be done in either order.

## Estimated scope

Moderate complexity. ~50-100 lines, but requires careful reasoning about the
backward-walk invariants.
