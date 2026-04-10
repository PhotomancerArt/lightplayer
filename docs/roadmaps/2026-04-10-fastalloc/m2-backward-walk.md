# Milestone 2: Backward-Walk Allocator (Straight-Line Code)

## Goal

Implement the core fastalloc algorithm: a backward-walk register allocator that
processes a single basic block (no control flow) and produces a
`FastAllocation` with per-operand assignments and explicit move edits. Call
clobber handling is part of the allocator — no more emitter-side call-save
machinery.

## Suggested plan name

`fastalloc-m2`

## Scope

### In scope

- `regalloc/fastalloc.rs`: new allocator module
- Backward walk over VInsts within a single block
- Per-instruction operand allocation (uses get registers, defs free registers)
- LRU eviction to spill slots when registers run out
- Call clobber handling: at call instructions, evict all live values in
  caller-saved registers to their spill slots
- Fixed-register constraints for call arguments (a0-a7) and returns (a0-a1)
- Spill slot allocation (lazy, on first eviction)
- Rematerialization of `IConst32` values (no spill slot needed)
- Produces `FastAllocation` consumed by the M1 emitter
- Config flag (`USE_FASTALLOC`) to select fastalloc vs greedy/linear
- Targeted filetests for straight-line code: call-clobber patterns, spill/reload
  sequences, high-pressure register usage

### Out of scope

- Multiple basic blocks, branches, labels (M3)
- Loop handling (M3)
- Block-boundary liveness reconciliation (M3)
- Performance benchmarking (M4)
- Param-to-callee-saved optimization (future improvement)

## Key Decisions

- The allocator walks instructions in reverse. At each instruction:
  1. Process defs (free the register, insert stack write if value was live)
  2. Process uses (ensure value is in a register, insert stack read if evicted)
  3. Handle call clobbers (evict all caller-saved regs with live values)
  4. Handle fixed-reg constraints (call args/rets)

- LRU eviction: when no free register is available, evict the least-recently-
  used vreg to its spill slot. Simple circular buffer over the ~15 allocatable
  registers.

- `MiniAllocState` data structures as described in the design doc:
  `vreg_home`, `preg_occupant`, `live` set, `vreg_spill_slot`, `lru`, `edits`.

- For this milestone, the allocator treats the entire VInst sequence as one
  block. Functions with control flow will produce incorrect results — that's
  fine, as M3 adds block splitting.

## Deliverables

- `regalloc/fastalloc.rs`: ~300-400 lines
- `regalloc/mod.rs`: `FastAllocator` integrated into allocator selection
- `config.rs`: `USE_FASTALLOC` flag
- New filetests targeting straight-line allocation patterns
- Existing straight-line filetests pass with fastalloc

## Dependencies

- M1 (allocation output format + emitter edit splicing)

## Estimated Scope

~400-500 lines of new code (allocator + tests). This is the algorithmic core
of the effort.
