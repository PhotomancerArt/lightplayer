//! Unified profiling: collectors, session, and trace directory layout.

use crate::Memory;
use ::alloc::boxed::Box;
use ::alloc::string::{String, ToString};
use ::alloc::vec::Vec;
use serde::Serialize;
use std::any::Any;
use std::borrow::Cow;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

pub mod alloc;
pub mod cpu;
pub mod events;
pub mod perf_event;

pub use perf_event::{EVENT_PROFILE_END, EVENT_PROFILE_START, PerfEvent, PerfEventKind};

pub use crate::emu::cycle_model::InstClass;
pub use cpu::CpuCollector;

/// A symbol entry shared across profile metadata (`meta.json`).
#[derive(Debug, Clone, Serialize)]
pub struct TraceSymbol {
    pub name: String,
    pub addr: u32,
    pub size: u32,
}

/// Top-level fields written to `meta.json` by [`ProfileSession::new`].
#[derive(Debug, Clone, Serialize)]
pub struct SessionMetadata {
    pub schema_version: u32,
    pub timestamp: String,
    pub project: String,
    pub workload: String,
    pub note: Option<String>,
    pub clock_source: &'static str,
    pub mode: String,
    pub cycle_model: String,
    pub max_cycles: u64,
    pub cycles_used: u64,
    pub terminated_by: String,
    pub symbols: Vec<TraceSymbol>,
}

/// Read-only emulator surface passed to collectors during syscall handling.
pub struct EmuCtx<'a> {
    pub pc: u32,
    pub regs: &'a [i32; 32],
    pub cycle_count: u64,
    pub instruction_count: u64,
    pub memory: &'a Memory,
}

impl EmuCtx<'_> {
    /// Unwind the guest call stack and return addresses (faulting PC first, then return sites).
    ///
    /// Logic matches `Riscv32Emulator::unwind_backtrace` (frame-pointer walk); duplicated here
    /// so collectors can use it without borrowing the full emulator.
    pub fn unwind_backtrace(&self) -> Vec<u32> {
        unwind_backtrace_inner(self.pc, self.regs, self.memory)
    }
}

/// Maximum number of frames to unwind to avoid runaway on corrupted stacks.
const MAX_FRAMES: usize = 32;

/// RISC-V RAM start (stack lives in RAM).
const RAM_START: u32 = 0x8000_0000;

fn unwind_backtrace_inner(pc: u32, regs: &[i32; 32], mem: &Memory) -> Vec<u32> {
    let mut addrs = Vec::with_capacity(MAX_FRAMES);
    let ram_end = mem.ram_end();

    addrs.push(pc);

    let ra = regs[1] as u32;
    if is_valid_code_address(ra, mem) {
        addrs.push(ra);
    }

    let mut fp = regs[8] as u32;
    if fp >= RAM_START && fp <= ram_end && fp % 4 == 0 {
        match mem.read_word(fp.wrapping_sub(8)) {
            Ok(pfp) => {
                if (pfp as u32) >= RAM_START {
                    fp = pfp as u32;
                } else {
                    return addrs;
                }
            }
            _ => return addrs,
        }
    } else {
        return addrs;
    }

    let mut frame_count = addrs.len();
    while frame_count < MAX_FRAMES {
        if fp < RAM_START || fp > ram_end || fp % 4 != 0 {
            break;
        }

        let saved_ra = match mem.read_word(fp.wrapping_sub(4)) {
            Ok(v) => v as u32,
            Err(_) => break,
        };
        let prev_fp = match mem.read_word(fp.wrapping_sub(8)) {
            Ok(v) => v,
            Err(_) => break,
        };

        if is_valid_code_address(saved_ra, mem) {
            addrs.push(saved_ra);
        }

        let prev_fp_u32 = prev_fp as u32;
        if prev_fp_u32 < RAM_START || prev_fp_u32 <= fp {
            break;
        }
        fp = prev_fp_u32;
        frame_count += 1;
    }

    addrs
}

fn is_valid_code_address(addr: u32, mem: &Memory) -> bool {
    if addr == 0 {
        return false;
    }
    if addr >= RAM_START {
        return false;
    }
    let code_start = mem.code_start();
    let offset = addr.wrapping_sub(code_start) as usize;
    offset < mem.code().len()
}

/// What the emulator should do after a collector handles a syscall.
pub enum SyscallAction {
    Pass,
    Handled,
    Halt(HaltReason),
}

/// Reasons the run loop may stop at collector request.
pub enum HaltReason {
    Oom { size: u32 },
    ProfileStop,
}

/// What a gate wants the session to do after observing an event.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum GateAction {
    NoChange,
    /// m1: logged only; m2 wires real enable/disable
    Enable,
    /// m1: logged only
    Disable,
    /// m1: triggers [`HaltReason::ProfileStop`]
    Stop,
}

/// Trait implemented by `ProfileMode` state machines (in lp-cli).
/// Lives here so [`ProfileSession`] can hold a `Box<dyn Gate>` without
/// a circular dep.
pub trait Gate: Send {
    fn on_event(&mut self, evt: &PerfEvent) -> GateAction;
    /// Called once at session end; lets gates emit a summary line
    /// into the report. Default: no-op.
    fn report_section(&self, _w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        Ok(())
    }
}

/// Context passed to [`Collector::finish`].
pub struct FinishCtx<'a> {
    pub trace_dir: &'a Path,
}

/// Resolve guest PCs to labels for profile reports (implemented in `lp-cli` via ELF symbols).
pub trait PcSymbolizer {
    fn symbolize(&self, pc: u32) -> Cow<'_, str>;

    /// Bucket a call-site PC into a stable "containing symbol" lower bound (defaults to `pc`).
    fn entry_lo_for_pc(&self, pc: u32) -> u32 {
        let _ = self;
        pc
    }
}

/// One enabled trace sink (alloc, cpu, …).
pub trait Collector: Send + Any {
    fn as_any(&self) -> &dyn Any;

    fn as_any_mut(&mut self) -> &mut dyn Any;

    fn name(&self) -> &'static str;

    fn report_title(&self) -> &'static str {
        self.name()
    }

    fn meta_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    fn on_syscall(&mut self, _ctx: &mut EmuCtx<'_>, _id: u32, _args: &[u32]) -> SyscallAction {
        SyscallAction::Pass
    }

    /// Called by [`ProfileSession::on_perf_event`] after running the gate.
    fn on_gate_action(&mut self, _action: GateAction) {}

    /// Called once per executed instruction when a profile session is active.
    ///
    /// `target_pc` is the next PC after this instruction
    /// (`new_pc` from the decoder, or `pc + inst_size` when `new_pc` is `None`).
    fn on_instruction(&mut self, _pc: u32, _target_pc: u32, _class: InstClass, _cycles: u32) {}

    fn on_perf_event(&mut self, _evt: &PerfEvent) {}

    fn finish(&mut self, ctx: &FinishCtx<'_>) -> std::io::Result<()>;

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result;

    /// Like [`Self::report_section`] but may substitute symbol names when `sym` is present.
    ///
    /// Default ignores `sym` and delegates to [`Self::report_section`].
    fn report_section_symbolized(
        &self,
        w: &mut dyn std::fmt::Write,
        sym: Option<&dyn PcSymbolizer>,
    ) -> std::fmt::Result {
        let _ = sym;
        self.report_section(w)
    }

    /// Events recorded by this collector since construction (after [`Self::finish`], still valid).
    fn event_count(&self) -> u64 {
        0
    }
}

/// Owns the trace directory, writes `meta.json`, dispatches syscalls, and builds `report.txt`.
pub struct ProfileSession {
    trace_dir: PathBuf,
    collectors: Vec<Box<dyn Collector>>,
    gate: Option<Box<dyn Gate>>,
    /// Sticky; first halt reason wins.
    halt_reason: Option<HaltReason>,
    /// Idempotent guard for [`ProfileSession::start`].
    started: bool,
}

impl ProfileSession {
    pub fn new(
        trace_dir: PathBuf,
        metadata: &SessionMetadata,
        collectors: Vec<Box<dyn Collector>>,
    ) -> std::io::Result<Self> {
        std::fs::create_dir_all(&trace_dir)?;

        let mut collectors_map = serde_json::Map::new();
        for c in &collectors {
            collectors_map.insert(c.name().to_string(), c.meta_json());
        }

        let mut meta_value = serde_json::json!({
            "schema_version": metadata.schema_version,
            "timestamp": metadata.timestamp,
            "project": metadata.project,
            "workload": metadata.workload,
            "note": metadata.note,
            "clock_source": metadata.clock_source,
            "mode": metadata.mode,
            "cycle_model": metadata.cycle_model,
            "max_cycles": metadata.max_cycles,
            "cycles_used": metadata.cycles_used,
            "terminated_by": metadata.terminated_by,
            "symbols": metadata.symbols,
        });
        if let serde_json::Value::Object(ref mut obj) = meta_value {
            obj.insert(
                "collectors".to_string(),
                serde_json::Value::Object(collectors_map),
            );
        }

        let meta_path = trace_dir.join("meta.json");
        let meta_file = File::create(&meta_path)?;
        serde_json::to_writer_pretty(BufWriter::new(meta_file), &meta_value)?;

        Ok(Self {
            trace_dir,
            collectors,
            gate: None,
            halt_reason: None,
            started: false,
        })
    }

    pub fn set_gate(&mut self, gate: Box<dyn Gate>) {
        self.gate = Some(gate);
    }

    /// Emit a synthetic `profile:start` perf event once per session (idempotent).
    ///
    /// Intended when the guest run loop begins; gives collectors a timeline marker and gates a
    /// uniform boot hook for [`GateAction::Enable`].
    pub fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        let evt = PerfEvent {
            cycle: 0,
            name: perf_event::EVENT_PROFILE_START,
            kind: PerfEventKind::Instant,
        };
        self.on_perf_event(&evt);
    }

    /// Emit a synthetic `profile:end` perf event at `final_cycle` (every call; not idempotent).
    pub fn end(&mut self, final_cycle: u64) {
        let evt = PerfEvent {
            cycle: final_cycle,
            name: perf_event::EVENT_PROFILE_END,
            kind: PerfEventKind::Instant,
        };
        self.on_perf_event(&evt);
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
        let action = self
            .gate
            .as_mut()
            .map(|g| g.on_event(evt))
            .unwrap_or(GateAction::NoChange);

        match action {
            GateAction::NoChange => {}
            GateAction::Enable | GateAction::Disable => {
                // m1: log only; m2 wires real semantics via [`Collector::on_gate_action`].
                log::trace!(
                    "gate transition (m1: noop): {:?} @ cycle {}",
                    evt,
                    evt.cycle
                );
            }
            GateAction::Stop => {}
        }

        for c in &mut self.collectors {
            c.on_gate_action(action);
        }

        if matches!(action, GateAction::Stop) && self.halt_reason.is_none() {
            self.halt_reason = Some(HaltReason::ProfileStop);
            log::debug!(
                "gate requested stop @ cycle {} ({} {:?})",
                evt.cycle,
                evt.name,
                evt.kind
            );
        }
    }

    pub fn dispatch_instruction(&mut self, pc: u32, target_pc: u32, class: InstClass, cycles: u32) {
        for c in &mut self.collectors {
            c.on_instruction(pc, target_pc, class, cycles);
        }
    }

    pub fn dispatch_syscall(
        &mut self,
        ctx: &mut EmuCtx<'_>,
        id: u32,
        args: &[u32],
    ) -> SyscallAction {
        for c in &mut self.collectors {
            match c.on_syscall(ctx, id, args) {
                SyscallAction::Pass => continue,
                action => return action,
            }
        }
        SyscallAction::Pass
    }

    pub fn collectors(&self) -> &[Box<dyn Collector>] {
        &self.collectors
    }

    pub fn finish(&mut self) -> std::io::Result<Vec<(String, u64)>> {
        self.finish_with_symbolizer(None)
    }

    pub fn finish_with_symbolizer(
        &mut self,
        sym: Option<&dyn PcSymbolizer>,
    ) -> std::io::Result<Vec<(String, u64)>> {
        for c in &mut self.collectors {
            let ctx = FinishCtx {
                trace_dir: &self.trace_dir,
            };
            c.finish(&ctx)?;
        }

        let counts: Vec<(String, u64)> = self
            .collectors
            .iter()
            .map(|c| (c.name().to_string(), c.event_count()))
            .collect();

        use std::fmt::Write as _;

        let mut buf = String::new();
        for (i, c) in self.collectors.iter().enumerate() {
            if i > 0 {
                buf.push('\n');
            }
            writeln!(&mut buf, "=== {} ===", c.report_title())
                .expect("writing to String cannot fail");
            c.report_section_symbolized(&mut buf, sym)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            buf.push('\n');
        }

        let report_path = self.trace_dir.join("report.txt");
        std::fs::write(&report_path, buf.as_bytes())?;

        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn test_metadata() -> SessionMetadata {
        SessionMetadata {
            schema_version: 1,
            timestamp: "2026-01-01T00:00:00Z".into(),
            project: "test".into(),
            workload: "test".into(),
            note: None,
            clock_source: "emu_estimated",
            mode: "steady-render".into(),
            cycle_model: "esp32c6".into(),
            max_cycles: 0,
            cycles_used: 0,
            terminated_by: "running".into(),
            symbols: Vec::new(),
        }
    }

    struct NoopCollector;

    impl Collector for NoopCollector {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn name(&self) -> &'static str {
            "noop"
        }

        fn finish(&mut self, _: &FinishCtx<'_>) -> std::io::Result<()> {
            Ok(())
        }

        fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
            writeln!(w, "noop")
        }
    }

    #[test]
    fn session_creates_dir_and_meta() {
        let tmp = tempfile::tempdir().unwrap();
        let metadata = SessionMetadata {
            schema_version: 1,
            timestamp: "2026-01-01T00:00:00Z".into(),
            project: "test".into(),
            workload: "test".into(),
            note: None,
            clock_source: "emu_estimated",
            mode: "steady-render".into(),
            cycle_model: "esp32c6".into(),
            max_cycles: 0,
            cycles_used: 0,
            terminated_by: "running".into(),
            symbols: Vec::new(),
        };
        let collectors: Vec<Box<dyn Collector>> =
            Vec::from([Box::new(NoopCollector) as Box<dyn Collector>]);
        let mut session =
            ProfileSession::new(tmp.path().to_path_buf(), &metadata, collectors).unwrap();
        assert!(tmp.path().join("meta.json").exists());
        let counts = session.finish().unwrap();
        assert_eq!(counts, Vec::from([("noop".to_string(), 0u64)]));
        assert!(tmp.path().join("report.txt").exists());
    }

    struct CountingCollector {
        n: u32,
    }

    impl Collector for CountingCollector {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn name(&self) -> &'static str {
            "count"
        }

        fn on_perf_event(&mut self, _: &PerfEvent) {
            self.n += 1;
        }

        fn finish(&mut self, _: &FinishCtx<'_>) -> std::io::Result<()> {
            Ok(())
        }

        fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
            writeln!(w, "{}", self.n)
        }
    }

    struct StopOnSecond {
        seen: u32,
    }

    impl Gate for StopOnSecond {
        fn on_event(&mut self, _: &PerfEvent) -> GateAction {
            self.seen += 1;
            if self.seen == 2 {
                GateAction::Stop
            } else {
                GateAction::NoChange
            }
        }
    }

    #[test]
    fn session_dispatches_perf_event_and_records_stop() {
        let tmp = tempfile::tempdir().unwrap();
        let collectors: Vec<Box<dyn Collector>> =
            Vec::from([Box::new(CountingCollector { n: 0 }) as Box<dyn Collector>]);
        let mut s =
            ProfileSession::new(tmp.path().to_path_buf(), &test_metadata(), collectors).unwrap();
        s.set_gate(Box::new(StopOnSecond { seen: 0 }));
        let evt = PerfEvent {
            cycle: 1,
            name: "frame",
            kind: PerfEventKind::Begin,
        };
        s.on_perf_event(&evt);
        assert!(s.take_halt_reason().is_none());
        s.on_perf_event(&evt);
        assert!(matches!(
            s.take_halt_reason(),
            Some(HaltReason::ProfileStop)
        ));
        assert!(s.take_halt_reason().is_none());
    }

    type InsRecord = (u32, u32, InstClass, u32);

    /// Two collectors append into the same `Vec` so we can assert both saw `on_instruction`.
    struct InstructionTapShared {
        name: &'static str,
        shared: Arc<Mutex<Vec<InsRecord>>>,
    }

    impl Collector for InstructionTapShared {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn name(&self) -> &'static str {
            self.name
        }

        fn on_instruction(&mut self, pc: u32, target_pc: u32, class: InstClass, cycles: u32) {
            self.shared
                .lock()
                .expect("test mutex")
                .push((pc, target_pc, class, cycles));
        }

        fn finish(&mut self, _: &FinishCtx<'_>) -> std::io::Result<()> {
            Ok(())
        }

        fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
            writeln!(w, "{}", self.shared.lock().expect("test mutex").len())
        }
    }

    struct GateActionRecorder {
        shared: Arc<Mutex<Vec<GateAction>>>,
    }

    impl Collector for GateActionRecorder {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn name(&self) -> &'static str {
            "gate_rec"
        }

        fn on_gate_action(&mut self, action: GateAction) {
            self.shared.lock().expect("test mutex").push(action);
        }

        fn finish(&mut self, _: &FinishCtx<'_>) -> std::io::Result<()> {
            Ok(())
        }

        fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
            writeln!(w, "{}", self.shared.lock().expect("test mutex").len())
        }
    }

    struct AlwaysEnableGate;

    impl Gate for AlwaysEnableGate {
        fn on_event(&mut self, _: &PerfEvent) -> GateAction {
            GateAction::Enable
        }
    }

    struct PerfEventRecorder {
        events: Arc<Mutex<Vec<(&'static str, PerfEventKind, u64)>>>,
    }

    impl Collector for PerfEventRecorder {
        fn as_any(&self) -> &dyn Any {
            self
        }

        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }

        fn name(&self) -> &'static str {
            "perf_rec"
        }

        fn on_perf_event(&mut self, evt: &PerfEvent) {
            self.events
                .lock()
                .expect("test mutex")
                .push((evt.name, evt.kind, evt.cycle));
        }

        fn finish(&mut self, _: &FinishCtx<'_>) -> std::io::Result<()> {
            Ok(())
        }

        fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
            writeln!(w, "{}", self.events.lock().expect("test mutex").len())
        }
    }

    #[test]
    fn session_start_emits_profile_start_event() {
        let tmp = tempfile::tempdir().unwrap();
        let shared = Arc::new(Mutex::new(Vec::<(&'static str, PerfEventKind, u64)>::new()));
        let collectors: Vec<Box<dyn Collector>> = Vec::from([Box::new(PerfEventRecorder {
            events: Arc::clone(&shared),
        }) as Box<dyn Collector>]);
        let mut session =
            ProfileSession::new(tmp.path().to_path_buf(), &test_metadata(), collectors).unwrap();
        session.set_gate(Box::new(AlwaysEnableGate));
        session.start();
        session.start();
        let v = shared.lock().expect("test mutex");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, perf_event::EVENT_PROFILE_START);
        assert_eq!(v[0].1, PerfEventKind::Instant);
        assert_eq!(v[0].2, 0);
    }

    #[test]
    fn session_end_emits_profile_end_event_with_cycle() {
        let tmp = tempfile::tempdir().unwrap();
        let shared = Arc::new(Mutex::new(Vec::<(&'static str, PerfEventKind, u64)>::new()));
        let collectors: Vec<Box<dyn Collector>> = Vec::from([Box::new(PerfEventRecorder {
            events: Arc::clone(&shared),
        }) as Box<dyn Collector>]);
        let mut session =
            ProfileSession::new(tmp.path().to_path_buf(), &test_metadata(), collectors).unwrap();
        session.end(12_345);
        let v = shared.lock().expect("test mutex");
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, perf_event::EVENT_PROFILE_END);
        assert_eq!(v[0].1, PerfEventKind::Instant);
        assert_eq!(v[0].2, 12_345);
    }

    #[test]
    fn dispatch_instruction_fans_out_to_all_collectors() {
        let tmp = tempfile::tempdir().unwrap();
        let shared = Arc::new(Mutex::new(Vec::<InsRecord>::new()));
        let want = (0x1000u32, 0x1004u32, InstClass::Alu, 1u32);
        let collectors: Vec<Box<dyn Collector>> = Vec::from([
            Box::new(InstructionTapShared {
                name: "ins_tap_0",
                shared: Arc::clone(&shared),
            }) as Box<dyn Collector>,
            Box::new(InstructionTapShared {
                name: "ins_tap_1",
                shared: Arc::clone(&shared),
            }) as Box<dyn Collector>,
        ]);
        let mut session =
            ProfileSession::new(tmp.path().to_path_buf(), &test_metadata(), collectors).unwrap();

        session.dispatch_instruction(want.0, want.1, want.2, want.3);

        let v = shared.lock().expect("test mutex");
        assert_eq!(&v[..], &[want, want]);
    }

    #[test]
    fn on_perf_event_fans_out_gate_action() {
        let tmp = tempfile::tempdir().unwrap();
        let shared = Arc::new(Mutex::new(Vec::<GateAction>::new()));
        let collectors: Vec<Box<dyn Collector>> = Vec::from([Box::new(GateActionRecorder {
            shared: Arc::clone(&shared),
        }) as Box<dyn Collector>]);
        let mut session =
            ProfileSession::new(tmp.path().to_path_buf(), &test_metadata(), collectors).unwrap();
        session.set_gate(Box::new(AlwaysEnableGate));

        let evt = PerfEvent {
            cycle: 7,
            name: "frame",
            kind: PerfEventKind::Begin,
        };
        session.on_perf_event(&evt);

        let g = shared.lock().expect("test mutex");
        assert_eq!(&g[..], &[GateAction::Enable]);
    }
}
