# Phase 4 — Host-side profile extensions

Extend `lp-riscv-emu/src/profile/` with everything the syscall
handler (phase 5) and CLI (phase 7) will plug into:

- `PerfEvent` + `PerfEventKind` (host-side types).
- `EventsCollector` (writes `events.jsonl`).
- `Gate` trait + `GateAction` enum (mode-side hook).
- `HaltReason::ProfileStop` (graceful termination signal).
- `ProfileSession::set_gate(...)` and `ProfileSession::on_perf_event(...)`.

This phase introduces the *types and dispatch logic*; the syscall
handler that calls `on_perf_event` lives in phase 5.

This phase can run in parallel with phases 1 and 2 (disjoint files).

## Subagent assignment

`generalPurpose` subagent. New module files + targeted edits to
`profile/mod.rs`. All shapes are pinned in the design doc — this is
mechanical translation.

## Files to create / update

```
lp-riscv/lp-riscv-emu/src/profile/
├── mod.rs               # UPDATE: + HaltReason variant, + Gate trait,
│                        #         + GateAction enum, + PerfEventKind
│                        #         re-export, + ProfileSession fields
│                        #         + set_gate + on_perf_event
├── perf_event.rs        # NEW
└── events.rs            # NEW

lp-riscv/lp-riscv-emu/Cargo.toml   # UPDATE: ensure serde_json
                                   # already present (it is, via
                                   # alloc.rs); no edit if so
```

## Contents

### `profile/perf_event.rs`

Per design doc, "Host-side `PerfEvent` and `PerfEventKind`" section.
Verbatim. Includes:

- `pub const MAX_EVENT_NAME_LEN: usize = 64`
- `pub static KNOWN_EVENT_NAMES: &[&str]` (the four names)
- `pub fn intern_known_name(s: &str) -> Option<&'static str>` — linear
  scan over `KNOWN_EVENT_NAMES`, returns `Some(static_str)` on hit.
- `pub struct PerfEvent { pub cycle: u64, pub name: &'static str, pub kind: PerfEventKind }`
- `pub enum PerfEventKind { Begin, End, Instant }` with `from_u32` and
  `as_str` helpers.
- Unit tests:
  - `PerfEventKind::from_u32` round-trips for 0/1/2 and rejects 3.
  - `intern_known_name("frame")` returns Some, `intern_known_name("xyz")`
    returns None, returned `&'static str` equals `EVENT_FRAME` literal.

### `profile/events.rs`

Per design doc, "`EventsCollector`" section. Verbatim. Key shape:

```rust
pub struct EventsCollector {
    writer: Option<BufWriter<File>>,    // None until file created
    path: PathBuf,                      // <trace_dir>/events.jsonl
    count: u64,
}

impl EventsCollector {
    pub fn new(trace_dir: &Path) -> std::io::Result<Self> {
        let path = trace_dir.join("events.jsonl");
        let file = File::create(&path)?;
        Ok(Self { writer: Some(BufWriter::new(file)), path, count: 0 })
    }
    pub fn count(&self) -> u64 { self.count }
}

impl Collector for EventsCollector {
    fn name(&self) -> &'static str { "events" }
    fn report_title(&self) -> &'static str { "Perf Events" }
    fn meta_json(&self) -> serde_json::Value {
        serde_json::json!({ "path": "events.jsonl", "format": "jsonl" })
    }
    fn on_perf_event(&mut self, evt: &PerfEvent) {
        if let Some(w) = self.writer.as_mut() {
            let line = serde_json::json!({
                "cycle": evt.cycle,
                "name":  evt.name,
                "kind":  evt.kind.as_str(),
            });
            // Best-effort write; log on error.
            if let Err(e) = serde_json::to_writer(&mut *w, &line)
                .and_then(|_| w.write_all(b"\n").map_err(Into::into))
            {
                log::warn!("EventsCollector write failed: {e}");
            } else {
                self.count += 1;
            }
        }
    }
    fn finish(&mut self, _ctx: &FinishCtx) -> std::io::Result<()> {
        if let Some(mut w) = self.writer.take() {
            w.flush()?;
        }
        Ok(())
    }
    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        writeln!(w, "events written: {}", self.count)?;
        writeln!(w, "path: events.jsonl")
    }
}
```

Unit tests:
- `new` creates the file.
- `on_perf_event` writes a JSON line and increments count.
- `finish` flushes (file content visible after `finish` returns).

(Use `tempfile::tempdir` like phase-1 m0 tests.)

### `profile/mod.rs` updates

1. **New module declarations**, near the top:
   ```rust
   pub mod alloc;
   pub mod events;          // NEW
   pub mod perf_event;      // NEW

   pub use perf_event::{PerfEvent, PerfEventKind};   // NEW re-export
   ```

2. **`HaltReason` extension** — add the new variant:
   ```rust
   pub enum HaltReason {
       Oom { size: u32 },
       ProfileStop,            // NEW
   }
   ```

3. **`PerfEvent` struct removal** — the m0 placeholder
   `pub struct PerfEvent {}` in `mod.rs` is now replaced by the
   re-export from `perf_event`. Delete the placeholder.

4. **`Gate` trait + `GateAction`** — new top-level definitions in
   `mod.rs`:
   ```rust
   /// What a gate wants the session to do after observing an event.
   #[derive(Copy, Clone, Debug, PartialEq, Eq)]
   pub enum GateAction {
       NoChange,
       Enable,    // m1: logged only; m2 wires real enable/disable
       Disable,   // m1: logged only
       Stop,      // m1: triggers HaltReason::ProfileStop
   }

   /// Trait implemented by `ProfileMode` state machines (in lp-cli).
   /// Lives here so `ProfileSession` can hold a `Box<dyn Gate>` without
   /// a circular dep.
   pub trait Gate: Send {
       fn on_event(&mut self, evt: &PerfEvent) -> GateAction;
       /// Called once at session end; lets gates emit a summary line
       /// into the report. Default: no-op.
       fn report_section(&self, _w: &mut dyn std::fmt::Write) -> std::fmt::Result {
           Ok(())
       }
   }
   ```

5. **`ProfileSession` extensions** — add a gate slot and the
   `on_perf_event` dispatch method:
   ```rust
   pub struct ProfileSession {
       trace_dir: PathBuf,
       collectors: Vec<Box<dyn Collector>>,
       gate: Option<Box<dyn Gate>>,           // NEW
       halt_reason: Option<HaltReason>,       // NEW (sticky; first wins)
   }

   impl ProfileSession {
       pub fn set_gate(&mut self, gate: Box<dyn Gate>) {
           self.gate = Some(gate);
       }

       /// Take the first halt reason produced during the session, if any.
       /// Returns None if no gate ever requested a stop.
       pub fn take_halt_reason(&mut self) -> Option<HaltReason> {
           self.halt_reason.take()
       }

       /// Non-destructive peek at the pending halt reason. Used by the
       /// run-loop syscall handler (phase 5) to check whether a stop
       /// was requested without consuming it.
       pub fn pending_halt_reason(&self) -> Option<&HaltReason> {
           self.halt_reason.as_ref()
       }

       /// Dispatch a perf event to all collectors and the gate.
       /// Called by the syscall handler (phase 5).
       pub fn on_perf_event(&mut self, evt: &PerfEvent) {
           for c in &mut self.collectors {
               c.on_perf_event(evt);
           }
           if let Some(g) = self.gate.as_mut() {
               match g.on_event(evt) {
                   GateAction::NoChange => {}
                   GateAction::Enable | GateAction::Disable => {
                       // m1: log only; m2 wires real semantics.
                       log::trace!(
                           "gate transition (m1: noop): {:?} @ cycle {}",
                           evt, evt.cycle
                       );
                   }
                   GateAction::Stop => {
                       if self.halt_reason.is_none() {
                           self.halt_reason = Some(HaltReason::ProfileStop);
                           log::debug!(
                               "gate requested stop @ cycle {} ({} {:?})",
                               evt.cycle, evt.name, evt.kind
                           );
                       }
                   }
               }
           }
       }
   }
   ```

6. **`ProfileSession::new`** — initialize the new fields:
   ```rust
   gate: None,
   halt_reason: None,
   ```

7. **`Collector` trait** — confirm `on_perf_event` default impl
   exists (added in m0 design); if not, add:
   ```rust
   pub trait Collector: Send {
       /* existing methods */
       fn on_perf_event(&mut self, _evt: &PerfEvent) {}
   }
   ```

## Validation

```bash
cargo check -p lp-riscv-emu
cargo build -p lp-riscv-emu
cargo test  -p lp-riscv-emu profile::

# All existing m0 tests must still pass.
cargo test  -p lp-riscv-emu
```

New unit tests covered:
- `profile::perf_event` — `PerfEventKind` round-trip, `intern_known_name`.
- `profile::events` — `EventsCollector` lifecycle (create/write/flush).
- `profile` (mod.rs) — `ProfileSession::on_perf_event` dispatches to
  collectors and to the gate; `GateAction::Stop` populates
  `halt_reason`; `take_halt_reason` returns and clears.

Sketch for the dispatch test:
```rust
#[test]
fn session_dispatches_perf_event_and_records_stop() {
    let tmp = tempfile::tempdir().unwrap();
    struct CountingCollector { n: u32 }
    impl Collector for CountingCollector {
        fn name(&self) -> &'static str { "count" }
        fn on_perf_event(&mut self, _: &PerfEvent) { self.n += 1; }
        fn finish(&mut self, _: &FinishCtx) -> std::io::Result<()> { Ok(()) }
        fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
            writeln!(w, "{}", self.n)
        }
    }
    struct StopOnSecond { seen: u32 }
    impl Gate for StopOnSecond {
        fn on_event(&mut self, _: &PerfEvent) -> GateAction {
            self.seen += 1;
            if self.seen == 2 { GateAction::Stop } else { GateAction::NoChange }
        }
    }
    let mut s = ProfileSession::new(
        tmp.path().to_path_buf(),
        &test_metadata(),
        vec![Box::new(CountingCollector { n: 0 })],
    ).unwrap();
    s.set_gate(Box::new(StopOnSecond { seen: 0 }));
    let evt = PerfEvent { cycle: 1, name: "frame", kind: PerfEventKind::Begin };
    s.on_perf_event(&evt);
    assert!(s.take_halt_reason().is_none());
    s.on_perf_event(&evt);
    assert!(matches!(s.take_halt_reason(), Some(HaltReason::ProfileStop)));
    // Sticky: take_halt_reason cleared it.
    assert!(s.take_halt_reason().is_none());
}
```

## Out of scope for this phase

- Syscall handler for `SYSCALL_PERF_EVENT` (phase 5).
- `StepResult::ProfileStop` / `run_until_yield_or_stop` (phase 5).
- Concrete `Gate` implementations (phase 6).
- CLI wire-up (phase 7).
