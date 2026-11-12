# Phase 5 — Emulator run-loop integration

Plumb the perf-event syscall through the emulator's run loop and
expose `run_until_yield_or_stop` for the CLI workload to use.

After this phase, a fw-emu binary issuing `SYSCALL_PERF_EVENT` will
land in `ProfileSession::on_perf_event`, the gate will see the event,
and `GateAction::Stop` will propagate up through the run loop into
the CLI as `WorkloadOutcome::ProfileStopped`.

This phase can run in parallel with phases 3 and 6.

## Subagent assignment

`generalPurpose` subagent. Touches three files in `lp-riscv-emu/src/emu/emulator/`,
each edit small and localized per the design doc.

## Files to update

```
lp-riscv/lp-riscv-emu/src/emu/emulator/
├── types.rs         # UPDATE: + StepResult::ProfileStop
├── run_loops.rs     # UPDATE: + SYSCALL_PERF_EVENT handler
└── state.rs         # UPDATE: + FrameOutcome enum,
                     #         + run_until_yield_or_stop method
```

(The existing `state.rs` already holds `Riscv32Emulator`'s methods
like `step` / `advance_time`; add the new method there. If the actual
file layout differs, place per local convention — the design doc's
references are illustrative.)

## Edits

### `types.rs` — `StepResult::ProfileStop`

Find the existing `StepResult` enum (likely already has variants like
`Continue`, `Halted`, `Yielded`, etc.). Add:

```rust
pub enum StepResult {
    /* existing variants */
    /// The active profile session's gate requested termination.
    /// The CLI should drain remaining buffers and finish the session.
    ProfileStop,
}
```

If `StepResult` is non-`Copy`, `ProfileStop` is unit and stays trivial.
Update any existing exhaustive `match`es on `StepResult` (search:
`rg "match.*StepResult|StepResult::" lp-riscv-emu/src/`) — add a
`StepResult::ProfileStop => /* propagate */` arm. For arms inside
`step()` itself, treat it like `Halted`: stop the inner instruction
loop and surface the result.

### `run_loops.rs` — `SYSCALL_PERF_EVENT` handler

Locate the syscall dispatch — there's an existing match on syscall
number for `SYSCALL_ALLOC_TRACE`. Add a new arm before the fallback:

```rust
use lp_riscv_emu_shared::syscall::SYSCALL_PERF_EVENT;
use crate::profile::{PerfEvent, PerfEventKind, perf_event::intern_known_name};

// inside handle_syscall(...) match:
SYSCALL_PERF_EVENT => {
    // ABI: a0 = name_ptr, a1 = name_len, a2 = kind (0/1/2),
    //      a3 = reserved (ignored in m1).
    let name_ptr = self.regs[10];      // a0
    let name_len = self.regs[11];      // a1
    let kind_raw = self.regs[12];      // a2

    let kind = match PerfEventKind::from_u32(kind_raw) {
        Some(k) => k,
        None => {
            log::warn!("SYSCALL_PERF_EVENT: invalid kind {kind_raw}");
            return SyscallAction::Handled;
        }
    };
    if name_len == 0 || name_len as usize > MAX_EVENT_NAME_LEN {
        log::warn!("SYSCALL_PERF_EVENT: bad name_len {name_len}");
        return SyscallAction::Handled;
    }
    // Read name bytes from guest memory. Use existing memory-read helper
    // (likely `self.memory.read_bytes(addr, len)` -> Vec<u8> or &[u8]).
    let bytes = match self.memory.read_bytes(name_ptr, name_len) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("SYSCALL_PERF_EVENT: memory read failed: {e}");
            return SyscallAction::Handled;
        }
    };
    let name_str = match core::str::from_utf8(&bytes) {
        Ok(s) => s,
        Err(_) => {
            log::warn!("SYSCALL_PERF_EVENT: name not utf8");
            return SyscallAction::Handled;
        }
    };
    let interned = match intern_known_name(name_str) {
        Some(s) => s,
        None => {
            // Unknown event name: drop with one warning per name.
            // (Per design doc: "matched against KNOWN_EVENT_NAMES;
            // unknown drops with a one-time warning per name").
            // m1 implementation: just log every time; one-time
            // de-dup is a deferred polish.
            log::warn!("SYSCALL_PERF_EVENT: unknown name {name_str:?}");
            return SyscallAction::Handled;
        }
    };

    let evt = PerfEvent {
        cycle: self.cycle_count,    // or whatever the field is named
        name: interned,
        kind,
    };
    if let Some(session) = self.profile_session.as_mut() {
        session.on_perf_event(&evt);
        // Surface ProfileStop immediately if the gate set it.
        if let Some(_reason) = session.take_halt_reason() {
            // Don't take it for real — the run loop checks again.
            // Restore by not actually consuming. (Design alt: peek.)
            // Simplest: re-set and let `step` see it next iter.
            session.request_stop();   // see helper below
        }
    }
    SyscallAction::Handled
}
```

Add a small helper on `ProfileSession` (in phase 4? — no, simplest
to add it here in phase 5 since it's run-loop specific) that re-sets
the `halt_reason` without consuming:

Actually, simpler: don't `take_halt_reason()` here. Just check it
non-destructively. Add to `ProfileSession`:

```rust
pub fn pending_halt_reason(&self) -> Option<&HaltReason> {
    self.halt_reason.as_ref()
}
```

(This addition belongs in phase 4 — fold it in there. Update phase 4
to include this getter alongside `take_halt_reason`.)

The handler then becomes:

```rust
if let Some(session) = self.profile_session.as_mut() {
    session.on_perf_event(&evt);
    if session.pending_halt_reason().is_some() {
        // Tell the inner step loop to stop after this instruction.
        self.profile_stop_pending = true;
    }
}
SyscallAction::Handled
```

Add `profile_stop_pending: bool` to the emulator state struct
(initialize false). The `step()` loop checks it and returns
`StepResult::ProfileStop` when set.

### `state.rs` — `FrameOutcome` + `run_until_yield_or_stop`

Add a new outcome enum (not in `types.rs` because it's CLI-facing
control flow, not raw step result):

```rust
/// Result of running one driven frame.
pub enum FrameOutcome {
    /// Guest yielded back to host (idle/scheduler block).
    Yielded,
    /// Profile gate requested stop.
    ProfileStop,
    /// Halted for any other reason (OOM, exit).
    Halted(HaltReason),
}
```

(`HaltReason` re-exported from `crate::profile` if not already in
scope.)

Add the method:

```rust
impl Riscv32Emulator {
    /// Drive the guest until it yields or the profile session stops.
    /// `max_steps` caps wall-clock per call to avoid runaway hangs.
    pub fn run_until_yield_or_stop(&mut self, max_steps: u64) -> FrameOutcome {
        let mut steps = 0u64;
        loop {
            if steps >= max_steps {
                // Safety cap: treat as a yield so the caller can
                // re-tick the clock. Don't surface as halt — caller
                // decides whether to continue.
                return FrameOutcome::Yielded;
            }
            match self.step() {
                StepResult::Continue => { steps += 1; }
                StepResult::Yielded => return FrameOutcome::Yielded,
                StepResult::ProfileStop => return FrameOutcome::ProfileStop,
                StepResult::Halted(reason) => return FrameOutcome::Halted(reason),
                /* match remaining variants per existing enum */
            }
        }
    }
}
```

If `StepResult` doesn't currently carry a `HaltReason` payload (m0
left it stubbed), pass through whatever the existing convention is —
worst case, `FrameOutcome::Halted(HaltReason::Oom { size: 0 })` as
a placeholder is acceptable for m1 since the only halt reasons in
play today are OOM (already triggers via alloc collector path) and
the new `ProfileStop`.

Pick a concrete `MAX_STEPS_PER_FRAME` constant in the emulator's
public API (or expose `max_steps` as a parameter only — let phase 7
choose). Recommendation: expose as parameter, no constant in the
emulator. The CLI knows about budgets, the emulator doesn't.

## Validation

```bash
cargo check -p lp-riscv-emu
cargo build -p lp-riscv-emu
cargo test  -p lp-riscv-emu

# Workspace-wide compile to catch break in fw-tests / lp-cli:
cargo check --workspace
```

New unit tests in this phase:
- `step()` returns `StepResult::ProfileStop` after `profile_stop_pending`
  is set.
- `run_until_yield_or_stop` returns `FrameOutcome::ProfileStop` when the
  step loop produces `StepResult::ProfileStop`.
- `run_until_yield_or_stop(max_steps=0)` returns `FrameOutcome::Yielded`
  immediately (cap test).

End-to-end syscall test (host issues a SYSCALL_PERF_EVENT) is covered
by phase 8 (e2e); a unit test here would require building a tiny
guest stub which isn't worth it.

## Out of scope for this phase

- Concrete `Gate` impls (phase 6).
- CLI wire-up (phase 7).
- `events.jsonl` end-to-end check (phase 8).
- Performance overhead measurement (m1 acceptance criteria; phase 8
  validation).
