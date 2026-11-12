# Phase 5 — `profile:start`/`profile:end` events + gate updates

Add `EVENT_PROFILE_START` and `EVENT_PROFILE_END` perf-event constants
to `lp-riscv-emu/src/profile/perf_event.rs`. Add
`ProfileSession::start()` and `end()` methods that emit synthetic
events through the normal `on_perf_event` path. Wire the calls into
the emulator at session boot and finish.

Patch m1's gate impls (`mode/compile.rs`, `mode/startup.rs`,
`mode/all.rs`) to fire `GateAction::Enable` on `profile:start`. (The
`SteadyRenderGate` is unchanged — it already fires `Enable` after
warmup independently of `profile:start`.)

**Manual review** — patches m1 code post-merge.

## Dependencies

- **P3** — needs `on_gate_action` fan-out in `ProfileSession::on_perf_event`.
- **m1 fully merged** — needs `mode/compile.rs`, `mode/startup.rs`,
  `mode/all.rs`, `mode/steady_render.rs` and the `EVENT_*` constants
  module.

## Files

### `lp-riscv-emu/src/profile/perf_event.rs` (m1)

Add two constants alongside the m1-defined `EVENT_FRAME`,
`EVENT_SHADER_COMPILE`, etc.:

```rust
pub const EVENT_PROFILE_START: &str = "profile:start";
pub const EVENT_PROFILE_END:   &str = "profile:end";
```

Add to `KNOWN_EVENT_NAMES` (or whatever the m1 validation set is
called):

```rust
pub const KNOWN_EVENT_NAMES: &[&str] = &[
    EVENT_PROFILE_START,
    EVENT_PROFILE_END,
    EVENT_FRAME,
    EVENT_SHADER_COMPILE,
    // ...
];
```

(Confirm exact constant names against m1's final shape; m1 designs
this set in its Phase 2/3.)

### `lp-riscv-emu/src/profile/mod.rs`

Add two methods on `ProfileSession`:

```rust
impl ProfileSession {
    /// Called by Riscv32Emulator immediately before the first instruction runs.
    /// Emits a synthetic profile:start perf event so:
    ///  - events.jsonl has a clear "session began" marker
    ///  - gates have a uniform "boot" hook to fire Enable from
    pub fn start(&mut self) {
        self.on_perf_event(PerfEvent {
            name: EVENT_PROFILE_START,
            kind: PerfEventKind::Instant,
            cycle: 0,
        });
    }

    /// Called by Riscv32Emulator::finish_profile_session before draining.
    pub fn end(&mut self, final_cycle: u64) {
        self.on_perf_event(PerfEvent {
            name: EVENT_PROFILE_END,
            kind: PerfEventKind::Instant,
            cycle: final_cycle,
        });
    }
}
```

### `lp-riscv-emu/src/emu/emulator/mod.rs`

Wire the calls:

```rust
impl Riscv32Emulator {
    pub fn run(&mut self, ...) -> Result<...> {
        if let Some(session) = self.profile_session.as_mut() {
            session.start();           // [m2 NEW]
        }
        // ... existing run-loop dispatch ...
    }

    pub fn finish_profile_session(&mut self) -> Option<ProfileSession> {
        if let Some(session) = self.profile_session.as_mut() {
            session.end(self.cycle_count); // [m2 NEW]
        }
        self.profile_session.take()
    }
}
```

(Verify `run`'s entry point and `finish_profile_session`'s shape
against m1's final state — m1 may have factored these differently.
The principle: `start` fires before the first instruction; `end`
fires after the last instruction, before collectors' `finish`.)

### `lp-cli/src/commands/profile/mode/compile.rs`

```rust
impl Gate for CompileGate {
    fn evaluate(&mut self, evt: &PerfEvent) -> GateAction {
        match (evt.name, evt.kind) {
            (EVENT_PROFILE_START, _) => GateAction::Enable,            // [m2 NEW]
            (EVENT_SHADER_COMPILE, PerfEventKind::End) => {
                if self.saw_first_compile { GateAction::Stop }
                else { self.saw_first_compile = true; GateAction::NoChange }
            }
            _ => GateAction::NoChange,
        }
    }
}
```

### `lp-cli/src/commands/profile/mode/startup.rs`

```rust
impl Gate for StartupGate {
    fn evaluate(&mut self, evt: &PerfEvent) -> GateAction {
        match (evt.name, evt.kind) {
            (EVENT_PROFILE_START, _) => GateAction::Enable,            // [m2 NEW]
            (EVENT_FRAME, PerfEventKind::End) => {
                if self.first_frame_ended { GateAction::NoChange }
                else { self.first_frame_ended = true; GateAction::Stop }
            }
            _ => GateAction::NoChange,
        }
    }
}
```

### `lp-cli/src/commands/profile/mode/all.rs`

```rust
impl Gate for AllGate {
    fn evaluate(&mut self, evt: &PerfEvent) -> GateAction {
        match (evt.name, evt.kind) {
            (EVENT_PROFILE_START, _) => GateAction::Enable,            // [m2 NEW]
            _ => GateAction::NoChange,
        }
    }
}
```

### `lp-cli/src/commands/profile/mode/steady_render.rs`

**No change.** SteadyRenderGate fires `Enable` after warmup
independently. Adding `profile:start → Enable` would fire enable too
early. Leave it as m1 ships it.

## Tests

### Per-mode gate tests

Add one test case to each of `compile.rs#tests`, `startup.rs#tests`,
`all.rs#tests`:

```rust
#[test]
fn enables_on_profile_start() {
    let mut gate = CompileGate::new();
    let action = gate.evaluate(&PerfEvent {
        name: EVENT_PROFILE_START,
        kind: PerfEventKind::Instant,
        cycle: 0,
    });
    assert_eq!(action, GateAction::Enable);
}
```

`steady_render.rs#tests` — add a *negative* test:

```rust
#[test]
fn does_not_enable_on_profile_start() {
    let mut gate = SteadyRenderGate::new();
    let action = gate.evaluate(&PerfEvent {
        name: EVENT_PROFILE_START,
        kind: PerfEventKind::Instant,
        cycle: 0,
    });
    assert_eq!(action, GateAction::NoChange);
}
```

### `lp-riscv-emu/src/profile/mod.rs#tests`

```rust
#[test]
fn session_start_emits_profile_start_event() {
    // Build session with a recording test gate.
    // Call session.start().
    // Assert the gate received a PerfEvent with name == EVENT_PROFILE_START
    //   and kind == Instant and cycle == 0.
}

#[test]
fn session_end_emits_profile_end_event() {
    // Same shape; assert cycle is the final value passed.
}
```

### Existing m1 gate tests

All existing m1 gate tests must still pass — `profile:start` is
additive. None of the m1 tests issue a `profile:start` event, so
their behavior is unchanged.

## Risk + rollout

- **Risk**: m1's `EventsCollector` may reject events with unknown
  names. Confirm `EVENT_PROFILE_START` and `EVENT_PROFILE_END` are
  added to `KNOWN_EVENT_NAMES` *before* `ProfileSession::start()`
  fires (which happens at every session). Otherwise events.jsonl
  writes fail at session boot.
- **Risk**: `finish_profile_session` ordering. `session.end()` must
  fire *before* `session.collectors.iter_mut().for_each(|c|
  c.finish(...))`, otherwise CpuCollector won't have a chance to see
  the `profile:end` event (which it ignores anyway, but
  EventsCollector won't write it to events.jsonl). Audit the
  finish-time path explicitly.
- **Risk**: if `run` is called multiple times (rare but possible),
  `session.start()` fires again and re-`Enable`s a gate that may
  have already `Stop`'d. The gate impls naturally handle this
  because `Stop` is a one-way action and the `pending_halt` is set
  once. But to be safe, add a `started: bool` guard in
  `ProfileSession::start()` to make it idempotent.
- **Rollback**: revert the perf-event constants and the gate-impl
  changes; revert the `start`/`end` method additions and their
  call sites.

## Acceptance

- `cargo test -p lp-riscv-emu` passes.
- `cargo test -p lp-cli` passes.
- New per-mode gate tests pass.
- `lp-cli profile examples/basic --mode all --max-cycles 1000000`
  produces a non-empty `events.jsonl` with the first event being
  `profile:start` and the last being `profile:end` (manual smoke
  test; full integration test in P8).
