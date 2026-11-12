# Phase 3 — `Collector` trait widening + `ProfileSession::dispatch_instruction`

Extend the `Collector` trait with two changes:
1. Add `on_gate_action(&mut self, GateAction)` (default no-op).
2. Widen `on_instruction` from `(pc, class, cycles)` to
   `(pc, target_pc, class, cycles)`.

Add `ProfileSession::dispatch_instruction(...)` that fans out to every
collector. Wire the `after_execute` helper (P2) to call it. Update
`ProfileSession::on_perf_event` to fan out the gate's `GateAction` to
every collector after running the gate.

`AllocCollector` and `EventsCollector` get the default no-op
implementations of the new/widened methods. `EventsCollector` may want
to log gate transitions to `events.jsonl` — confirm with m1's
EventsCollector spec; if m1 already does this, no change here.

**Manual review** — touches m1's `Collector` trait shape and is the
highest-risk merge interaction with m1.

## Dependencies

- **P1** — needs the new `InstClass` enum imported into
  `profile/mod.rs`.
- **P2** — needs `after_execute` helper to wire the dispatch call.
- **m1 fully merged** — needs `Collector` trait, `Gate` trait,
  `GateAction` enum, `PerfEvent`, and `ProfileSession::on_perf_event`
  all in place.

## Files

### `lp-riscv-emu/src/profile/mod.rs`

Replace the m0-era stub `pub struct InstClass {}` (or whatever m1
leaves):

```rust
pub use crate::emu::cycle_model::InstClass;
```

Update the `Collector` trait:

```rust
pub trait Collector: Send {
    fn on_syscall(&mut self, _ctx: &mut EmuCtx, _id: u32, _args: &[u32; 8]) -> SyscallAction {
        SyscallAction::Forward
    }
    fn on_perf_event(&mut self, _event: &PerfEvent) {}

    /// Called by ProfileSession::on_perf_event after running the gate.
    /// Lets collectors react to Enable/Disable transitions.
    fn on_gate_action(&mut self, _action: GateAction) {}                  // [m2 NEW]

    /// Called once per instruction by the run-loop helper.
    /// `target_pc` is the next-PC after this instruction
    /// (computed by the helper from `new_pc.unwrap_or(pc + inst_size)`).
    fn on_instruction(&mut self, _pc: u32, _target_pc: u32,                // [m2 SIG WIDENED]
                       _class: InstClass, _cycles: u32) {}

    fn finish(&mut self, _ctx: &FinishCtx) -> std::io::Result<()> { Ok(()) }
    fn report_section(&self, _w: &mut dyn std::io::Write) -> std::io::Result<()> { Ok(()) }
}
```

Add the `dispatch_instruction` method on `ProfileSession`:

```rust
impl ProfileSession {
    pub fn dispatch_instruction(&mut self, pc: u32, target_pc: u32,
                                 class: InstClass, cycles: u32) {
        for c in &mut self.collectors {
            c.on_instruction(pc, target_pc, class, cycles);
        }
    }
}
```

Update `ProfileSession::on_perf_event` (m1 leaves this method in place;
m2 adds the gate-action fan-out):

```rust
pub fn on_perf_event(&mut self, event: PerfEvent) {
    for c in &mut self.collectors { c.on_perf_event(&event); }
    let action = self.gate.as_mut()
        .map(|g| g.evaluate(&event))
        .unwrap_or(GateAction::NoChange);
    for c in &mut self.collectors { c.on_gate_action(action); }   // [m2 NEW]
    if matches!(action, GateAction::Stop) {
        self.pending_halt = Some(HaltReason::ProfileStop);
    }
}
```

(The first three lines exist in m1; this phase only adds the
`for c in &mut self.collectors { c.on_gate_action(action); }`
line. Verify against m1's final shape before submitting.)

Add `Any` bound to `Collector` (needed by P6/P7's downcast for the
CpuCollector finish-time output writers):

```rust
use std::any::Any;
pub trait Collector: Send + Any {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    // ... existing methods ...
}
```

Each collector impl gains:
```rust
fn as_any(&self) -> &dyn Any { self }
fn as_any_mut(&mut self) -> &mut dyn Any { self }
```

### `lp-riscv-emu/src/profile/alloc.rs`

No code changes — defaults handle `on_gate_action` and the widened
`on_instruction`. Add the two `as_any`/`as_any_mut` methods.

### `lp-riscv-emu/src/profile/events.rs` (m1)

Same: defaults cover the new methods. Add `as_any`/`as_any_mut`.

If m1's `EventsCollector::on_perf_event` writes events to
`events.jsonl`, confirm `EVENT_PROFILE_START` / `EVENT_PROFILE_END`
are accepted by its event-name validation (P5 adds the constants;
events.jsonl writer should accept them as known names by then).

### `lp-riscv-emu/src/emu/emulator/mod.rs`

Update `Riscv32Emulator::after_execute` (added in P2) to call
`dispatch_instruction`:

```rust
#[inline(always)]
fn after_execute(&mut self, pc: u32, exec_result: &ExecutionResult) {
    let class = exec_result.class;
    let cost = self.cycle_model.cycles_for(class);
    self.cycle_count += cost as u64;
    if let Some(profile) = self.profile_session.as_mut() {
        let target_pc = exec_result.new_pc
            .unwrap_or(pc.wrapping_add(exec_result.inst_size as u32));
        profile.dispatch_instruction(pc, target_pc, class, cost);
    }
}
```

Drops the underscore on `pc`, adds the dispatch call.

## Tests

### `lp-riscv-emu/src/profile/mod.rs#tests`

Two new tests:

```rust
#[test]
fn dispatch_instruction_fans_out_to_all_collectors() {
    // Build a ProfileSession with two recording test collectors.
    // Call session.dispatch_instruction(0x1000, 0x1004, InstClass::Alu, 1).
    // Assert both collectors received the call with matching args.
}

#[test]
fn on_perf_event_fans_out_gate_action() {
    // Build a ProfileSession with one recording collector and a
    // test gate that returns Enable on any event.
    // Call session.on_perf_event(test_event()).
    // Assert collector saw on_gate_action(GateAction::Enable).
}
```

A `RecordingCollector` test fixture (records every callback with its
args into a Vec) is useful for both tests. If m1 already provides
one, reuse it.

### Integration

Confirm that `cargo test -p lp-riscv-emu` passes (no behavior change
for `AllocCollector` / `EventsCollector`).

## Risk + rollout

- **Highest-risk merge interaction with m1.** Coordinate with the m1
  branch's owner before submitting. Specifically: m1 may have
  finalized `Collector` with a slightly different shape (e.g., method
  ordering, `Sync` bound). Reconcile against m1's final shape.
- **`Any` bound impact**: adding `Any` to `Collector` makes every
  collector require `'static`. Existing impls (`AllocCollector`,
  `EventsCollector`) are already `'static` because they own their
  data; new collectors must be too. CpuCollector (P4) is fine.
- **Rollback**: revert the trait additions and the `dispatch_instruction`
  method. The `after_execute` change in P2 reverts cleanly to the
  pre-dispatch shape.

## Acceptance

- `cargo test -p lp-riscv-emu` passes.
- `cargo build -p lp-cli` succeeds (no callers broken; P6 wires the
  new collector).
- New unit tests in `profile/mod.rs#tests` pass.
- `rg 'fn on_instruction' lp-riscv-emu/` shows the widened signature
  in the trait and (after default usage) no manual impls.
