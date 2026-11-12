## Register Allocation Improvements

- Verifier: liveness-aware clobber safety check. Verify that every vreg live in
  a caller-saved pool reg across a VInst::Call has a matching save/restore edit
  pair (Move(reg→stack) at Before(call), Move(stack→reg) at After(call)). Deferred
  from M3.2 because it requires tracking per-instruction liveness in the verifier.

- Refactor call clobber handling to evict-then-reload (regalloc2 style).
  Currently we generate save/restore pairs in step 3 (clobber), then allocate
  args in step 4, which can evict vregs from the same registers. This creates
  an ordering hazard: the clobber save captures the wrong register contents in
  the forward direction when an eviction replaces the occupant. We patched this
  (remove save for evicted caller-saved regs, add explicit restore for evicted
  callee-saved regs), but regalloc2's fastalloc avoids the problem entirely by
  evicting all clobbered-reg occupants from the pool *before* arg allocation.
  The backward-walk equivalent: at a call, remove occupants from the pool and
  emit only a post-call reload (After: slot -> reg). No save needed — the
  eviction forces the def (reached later in the backward walk) to write to the
  spill slot. Eliminates the fixup logic and the whole class of ordering bugs.

- Adopt regalloc2-style operand constraints for cleaner sret/Ret handling.
  regalloc2 uses an `OperandConstraint` system (Any, Reg, FixedReg, Stack, etc.)
  where operands declare their requirements. When an operand has a `Stack`
  constraint and the value is already spilled, regalloc2 keeps it on the stack
  without reloading. Our current fix for the Ret operand collision (keeping
  spilled sret values on stack) achieves the same outcome but through
  special-case handling in `process_generic`. A constraint system would be more
  general and cleaner, allowing any instruction to declare which operands can
  stay spilled rather than hardcoding sret detection.

## Memory Improvements

pub clobbered: BTreeSet<PhysReg>, // todo: PRegSet??
vec use in general in lpvm-native
