## Register Allocation Improvements

- Verifier: liveness-aware clobber safety check. Verify that every vreg live in
  a caller-saved pool reg across a VInst::Call has a matching save/restore edit
  pair (Move(reg→stack) at Before(call), Move(stack→reg) at After(call)). Deferred
  from M3.2 because it requires tracking per-instruction liveness in the verifier.

## Memory Improvements

pub clobbered: BTreeSet<PhysReg>, // todo: PRegSet??
vec use in general in lpvm-native
