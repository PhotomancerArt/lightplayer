# Milestone 3: Control Flow and Calls

## Goal

Extend `fa_alloc` to handle IfThenElse, Loop, and Call instructions. All
filetests pass under `rv32fa`, including control flow and builtin calls.

## Suggested Plan Name

`fastalloc3-m3`

## Scope

### In scope

- **IfThenElse liveness**: merge `live_in` sets from then/else branches at the
  head; propagate `live_out` from successors
- **Loop liveness**: fixed-point iteration for loop header/body liveness
  convergence
- **IfThenElse allocation**: backward walk through else-body, then-body, head
  with register state merging at join points
- **Loop allocation**: walk body, then header, with register state convergence
- **Call clobbers**: spill live caller-saved registers before calls, reload after;
  place arguments in ABI registers; read return values from ABI registers
- **SRET handling**: preserve SRET pointer (s1) across calls for functions
  returning vec3/vec4
- **Seq region allocation**: walk children in reverse order, threading register
  state between them
- **Full filetest coverage**: all existing filetests pass under `rv32fa`,
  `unimplemented` annotations removed

### Out of scope

- Removing old cranelift pipeline — M4
- Optimization of allocation quality (callee-saved preferences, better
  heuristics) — future work
- Live range splitting — future work

## Key Decisions

- At IfThenElse join points, the allocator must reconcile register assignments
  from then/else branches. Strategy: pick one branch's assignment as canonical,
  emit moves at the end of the other branch to match.
- For loops, the allocator makes two passes: first pass determines which vregs
  are live across the loop back-edge, second pass allocates with that knowledge.
- Call handling follows RISC-V calling convention: a0-a7 for args, a0-a1 for
  returns, caller saves t0-t6/a0-a7.

## Deliverables

- Updated `fa_alloc/liveness.rs` with real IfThenElse and Loop liveness
- Updated `fa_alloc/walk.rs` with control flow and call allocation
- `fa_alloc/spill.rs` extended for call-related spills
- All filetests passing under `rv32fa`
- Unit tests for: if/else allocation, loop allocation, call with no live vars,
  call with spills, nested control flow

## Dependencies

- M2 (integration): `rv32fa` filetest target exists and straight-line tests pass

## Design Notes (from M1 planning)

### Calls in the backward walk

When the backward walk encounters `Call`:
- **Return values** (defs): call defines results in `RET_REGS`. Going backward,
  these are "born" here — free those PRegs.
- **Arguments** (uses): must be in `ARG_REGS`. Ensure vregs are in correct ABI
  registers; emit `Mv` if they're elsewhere.
- **Clobbers**: all caller-saved regs (a0-a7, t0-t6) are clobbered. Any live
  vreg in a caller-saved reg must be spilled before the call and reloaded after.
  In backward walk terms: emit reload (Lw) first (later in execution), then
  spill (Sw). The instruction stream reversal at the end puts them in the right
  order.

`RegPool` needs: "which caller-saved PRegs are currently occupied by live
vregs?" — filter `preg_vreg` against `FuncAbi::call_clobbers()`.

### Callee-saved registers

If the allocator assigns a vreg to a callee-saved register (s2-s11), that reg
must be saved in the prologue and restored in the epilogue. The allocator should:
- Track which callee-saved PRegs were actually used
- Report them in `AllocResult` (e.g. `callee_saved_used: Vec<PReg>`)
- FrameSetup/FrameTeardown emit save/restore only for used callee-saved regs

For straight-line no-call functions, callee-saved regs don't technically need
saving (no caller to preserve for). But the tracking should be in place from M1.

### Incoming args: register reuse

Params arrive in ARG_REGS. Once a param vreg is dead (no more uses), its ARG_REG
should be freed for reuse. In the backward walk, params are the last defs
encountered. When we first encounter a use of a param vreg, we ideally allocate
it to its ARG_REG directly (avoiding a fixup Mv). This is an optimization — M1
can do the naive thing (allocate from free list, fixup at end) and optimize
later by adding a "preferred register" hint.

### Caller-saved registers around calls

When `Call` clobbers caller-saved regs:
- Any live vreg in those regs → spill before call, reload on demand after
- `RegPool` clobber method: evict all caller-saved occupants, emit Sw for each
- Reloads happen lazily when the vreg is next used

## Estimated Scope

~600-1000 lines of new/modified code. This is the most complex milestone —
control flow merging and call clobbers are the hardest parts of register
allocation.
