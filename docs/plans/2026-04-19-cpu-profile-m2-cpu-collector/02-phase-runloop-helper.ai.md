# Phase 2 — `after_execute` helper in run loops

Factor the post-`decode_execute` work in `run_inner_fast` and
`run_inner_logging` into a single `#[inline(always)]` method on
`Riscv32Emulator`. Today both loops have an identical
`self.cycle_count += cost` line; m2 needs to add a
`profile_session.dispatch_instruction` call beside it. Inlining the
combined logic into a helper keeps both run loops in sync.

This phase **can ship in parallel with m1** — neither
`Riscv32Emulator` nor the run loops are scope for m1. The
`profile_session` field already exists on `main` (m0).

The helper's `dispatch_instruction` call is **dead code at this
phase** — `ProfileSession::dispatch_instruction` doesn't exist until
P3. To avoid breaking the build, P2 either:
- (preferred) gates the dispatch call behind `cfg(feature = "stub")`
  pseudocode that compiles to a no-op, OR
- defers adding the dispatch line until P3 lands and only ships the
  cycle-bump factoring in P2.

**Pick the second option** — ship the helper with only the cycle-bump
inside it; P3 adds the dispatch call. Smaller phases, no temporary
cfg gymnastics.

**Sub-agent suitable**: yes (mechanical refactor + run-loop test
remains green).

## Dependencies

- **P1** — needs `ExecutionResult.inst_size` field (P1 adds it).
  Without `inst_size` the helper can't compute `target_pc` for
  non-jump instructions. Even though P2 doesn't *use* `target_pc` yet
  (P3 wires the dispatch), P1's field is a compile-time prerequisite
  for the helper's signature stability.

  (Alternative: P2 doesn't take `inst_size` until P3. Rejected —
  introduces an interface change in P3 instead of P2, churn for
  nothing.)
- No m1 dependency.

## Files

### `lp-riscv-emu/src/emu/emulator/run_loops.rs`

Replace the two cycle-bump sites in `run_inner_fast` and
`run_inner_logging`:

**Before**:
```rust
let exec_result = decode_execute::<...>(inst_word, pc, &mut self.regs, &mut self.memory)?;
self.cycle_count += self.cycle_model.cycles_for(exec_result.class) as u64;
```

**After**:
```rust
let exec_result = decode_execute::<...>(inst_word, pc, &mut self.regs, &mut self.memory)?;
self.after_execute(pc, &exec_result);
```

### `lp-riscv-emu/src/emu/emulator/mod.rs` (or wherever `Riscv32Emulator` is `impl`'d)

```rust
impl Riscv32Emulator {
    #[inline(always)]
    fn after_execute(&mut self, _pc: u32, exec_result: &ExecutionResult) {
        let cost = self.cycle_model.cycles_for(exec_result.class);
        self.cycle_count += cost as u64;
        // P3 will add: profile_session.dispatch_instruction(pc, target_pc, exec_result.class, cost)
    }
}
```

`_pc` underscored because P2 doesn't use it yet. P3 drops the
underscore and adds the dispatch call.

## Tests

No new tests in P2. The factoring is provably-equivalent to the
inline code, and the existing run-loop tests
(`run_inner_fast_executes_simple_program`, etc.) cover both code
paths. Expect zero behavior change.

## Risk + rollout

- **Risk**: borrow checker. `after_execute` takes `&mut self`; the
  calling site has `exec_result: &ExecutionResult` borrowed but
  *not* from `self` (it was returned by value from `decode_execute`).
  Should compile cleanly. If it doesn't, the fix is to pass
  `exec_result` by value instead of reference (cheap — it's a small
  Copy-able struct except for the Option<InstLog>; check whether
  `ExecutionResult` is `Copy` after P1's field addition).
- **Rollback**: trivial. Inline the helper body back at both sites.
- **Hidden coupling**: any other run-loop variant (e.g., a fuzz-test
  loop, a bench harness) that re-implements the cycle bump should
  also call `after_execute`. Search `cycle_count.*+=` in
  `lp-riscv-emu/src/` and update.

## Acceptance

- All `cargo test -p lp-riscv-emu` passes.
- `rg 'cycle_count\s*\+=' lp-riscv-emu/src/` returns only the helper
  itself (one site).
- Run-loop benchmark (if one exists) shows no regression.
