# Milestone 3: Calls + Sret

## Goal

Extend the allocator and emitter to handle function calls, including caller-
save/restore, argument/return ABI, sret (return via pointer), and stack-passed
arguments. Call-related filetests pass.

## Suggested Plan Name

`fastalloc4-m3`

## Scope

### In scope

- **Call handling in backward walk:**
  - Return values (defs): allocate to RET_REGS, emit moves from ABI regs
    to allocated regs as edits
  - Arguments (uses): resolve to allocated regs, emit moves to ARG_REGS
    as edits before the call
  - Clobbers: all caller-saved regs. For each live vreg in a clobbered reg,
    record `Edit::Move(reg → slot)` before the call and `Edit::Move(slot → reg)`
    after the call
  - vmctx (v0/a0): always passed as first argument, handled by precolor system

- **Sret support:**
  - Callee returning via sret: set up a0 with buffer pointer, args shift to
    a1-a7, load return values from sret buffer post-call
  - Caller returning via sret: save sret pointer (s1), emit stores to sret
    buffer before ret

- **Stack-passed arguments:**
  - Outgoing: >8 args spill to stack at SP+offset before call
  - Incoming: load from caller's frame (FP-relative) at function entry

- **Unit tests (snapshot style):**
  - Simple call: one call with 1-2 args, 1 return
  - Call with live values across: vregs live before and after a call
  - Multi-arg call: 4-8 args
  - Sret call: callee returns vec2/vec4 via pointer
  - Stack args: call with >8 args

- **Filetest validation:** call-related filetests pass:
  - `native-call-simple.glsl`
  - `native-call-multi-args.glsl`
  - `native-call-vec2-return.glsl`
  - `native-call-nested.glsl`
  - `native-call-vec4-return.glsl` (sret)
  - `native-call-mat4-return.glsl` (sret)
  - `native-multi-function.glsl`

### Out of scope

- IfThenElse / Loop (M4)
- Filetests requiring control flow (native-call-control-flow.glsl)

## Key Decisions

- Call clobber spills are explicit edits (`Edit::Move` from reg to slot before
  the call, from slot to reg after). The emitter sees them as regular moves.

- The emitter's call emission (auipc+jalr pair, arg moves, ret moves) is
  already ported from `lpvm-native` in M1. M3 wires the allocator to produce
  correct edits around calls.

- Sret buffer allocation is part of the frame layout (already in `abi/frame.rs`).

## Deliverables

- Updated `fa_alloc/walk.rs` — call handling in backward walk
- Updated `rv32/emit.rs` — sret emission, stack arg emission (if not already
  complete from M1 port)
- Snapshot unit tests for call patterns
- Call-related filetests passing under `rv32fa`

## Dependencies

- M2 (straight-line): backward walk and alloc output working for simple cases

## Estimated Scope

~300-500 lines allocator additions, ~100 lines emitter adjustments, ~150 lines
tests.
