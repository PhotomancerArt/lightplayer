# CPU Profile m2 — CPU collector + outputs — Design

Implements `docs/roadmaps/2026-04-19-cpu-profile/m2-cpu-collector.md`
(CPU collector + Speedscope/canonical-JSON output) on top of m0's
collector skeleton and m1's perf-event/gate substrate.

Standalone deliverable:
`lp-cli profile examples/basic` (default `--collect cpu --mode
steady-render`) produces a callgrind-style flame chart of the
steady-state render.

See `00-notes.md` for resolved questions and rationale (R1–R16,
R-INIT). This design folds those decisions into a complete
specification.

## Architecture overview

```text
                     ┌──────────────────────────────────────┐
                     │ run_inner_fast / run_inner_logging   │
                     │   loop {                             │
                     │     decode_execute(pc) -> result     │
                     │     self.after_execute(pc, &result)  │ ← new helper
                     │   }                                  │
                     └─────────────────┬────────────────────┘
                                       ▼
                     ┌──────────────────────────────────────┐
                     │ Riscv32Emulator::after_execute       │
                     │  cycle_count += cost                 │
                     │  if let Some(profile) = ... {        │
                     │    profile.dispatch_instruction(...) │
                     │  }                                   │
                     └─────────────────┬────────────────────┘
                                       ▼
        ┌──────────────────────────────────────────────────────────┐
        │ ProfileSession                                           │
        │                                                          │
        │  on_perf_event(evt):                                     │
        │    for c in collectors: c.on_perf_event(&evt)            │
        │    action = gate.evaluate(&evt)                          │
        │    for c in collectors: c.on_gate_action(action)         │
        │    if action == Stop: pending_halt = ProfileStop         │
        │                                                          │
        │  dispatch_instruction(pc, target_pc, class, cycles):     │
        │    for c in collectors: c.on_instruction(...)            │
        │                                                          │
        │  start():                                                │
        │    self.on_perf_event(PerfEvent::profile_start(0))       │
        │  finish():                                               │
        │    self.on_perf_event(PerfEvent::profile_end(cycle))     │
        │    for c in collectors: c.finish(&ctx)?                  │
        └──────────────────────────────────────────────────────────┘
                                       │
              ┌────────────────────────┼────────────────────────┐
              ▼                        ▼                        ▼
   ┌──────────────────┐    ┌──────────────────┐    ┌──────────────────┐
   │ EventsCollector  │    │ AllocCollector   │    │ CpuCollector     │
   │ (m1)             │    │ (m0)             │    │ (m2 NEW)         │
   │                  │    │                  │    │                  │
   │ on_perf_event:   │    │ on_syscall:      │    │ on_gate_action:  │
   │   write JSONL    │    │   write trace    │    │   flip active    │
   │                  │    │                  │    │ on_instruction:  │
   │ (gate-agnostic)  │    │ (gate-agnostic)  │    │   if active:     │
   │                  │    │                  │    │     attribute    │
   │                  │    │                  │    │     cycles       │
   └──────────────────┘    └──────────────────┘    └──────────────────┘
                                                            │
                                                            ▼
                                              ┌──────────────────┐
                                              │ at finish() →    │
                                              │ output_speedscope│
                                              │ output_cpu_json  │
                                              │ report section   │
                                              └──────────────────┘
```

## File-level changes

### `lp-riscv-emu` (host emulator)

#### `src/emu/cycle_model.rs` — `InstClass` extension

Replace existing `InstClass::Jal` and `InstClass::Jalr` with five
call/return-aware variants. All other variants unchanged.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstClass {
    Alu, Mul, DivRem, Load, Store,
    BranchTaken, BranchNotTaken,
    JalCall,         // JAL rd, _   with rd != x0
    JalTail,         // JAL x0, _
    JalrCall,        // JALR rd, _, _   with rd != x0
    JalrReturn,      // JALR x0, x1, 0   (canonical `ret`)
    JalrIndirect,    // any other JALR x0, _, _
    Lui, Auipc, System, Fence, Atomic,
}

impl CycleModel {
    pub fn cycles_for(self, class: InstClass) -> u32 {
        match self {
            CycleModel::InstructionCount => 1,
            CycleModel::Esp32C6 => match class {
                // … unchanged costs for Alu, Mul, etc. …
                InstClass::JalCall  | InstClass::JalTail                       => 2,
                InstClass::JalrCall | InstClass::JalrReturn | InstClass::JalrIndirect => 3,
                // … other unchanged costs …
            },
        }
    }
}
```

`CycleModel::Esp32C6` keeps the existing per-mnemonic costs (Jal=2,
Jalr=3) for all five new variants — m2 adds no microarchitectural
differentiation. m4 may refine these later.

#### `src/emu/executor/jump.rs` — JAL/JALR classification

```rust
// In decode_execute_jal:
let class = if rd == 0 {
    InstClass::JalTail
} else {
    InstClass::JalCall
};

// In decode_execute_jalr:
let class = if rd != 0 {
    InstClass::JalrCall
} else if rs1 == 1 /* x1 = ra */ && imm == 0 {
    InstClass::JalrReturn
} else {
    InstClass::JalrIndirect
};
```

`rd`, `rs1`, `imm` are already decoded locally (see R1 of design
proposal — local placement chosen).

#### `src/emu/executor/compressed.rs` — compressed jump classification

| Mnemonic     | Encoding   | New class                                      |
| ------------ | ---------- | ---------------------------------------------- |
| `c.j`        | rd = x0    | `JalTail`                                      |
| `c.jal`      | rd = x1    | `JalCall`                                      |
| `c.jr rs1`   | rd = x0    | `JalrReturn` if `rs1 == 1` else `JalrIndirect` |
| `c.jalr rs1` | rd = x1    | `JalrCall`                                     |

#### `src/emu/executor/mod.rs` — `ExecutionResult`

Add `inst_size: u8` (2 for compressed, 4 for full). Decoder fills it.
Used by `after_execute` to derive `target_pc` for non-jump
instructions: `target_pc = pc + inst_size`.

```rust
pub struct ExecutionResult {
    pub new_pc: Option<u32>,
    pub should_halt: bool,
    pub syscall: bool,
    pub class: InstClass,
    pub inst_size: u8,           // [m2 NEW] 2 or 4
    pub log: Option<InstLog>,
}
```

#### `src/emu/emulator/run_loops.rs` — `after_execute` helper

```rust
impl Riscv32Emulator {
    #[inline(always)]
    fn after_execute(&mut self, pc: u32, exec_result: &ExecutionResult) {
        let class = exec_result.class;
        let cost  = self.cycle_model.cycles_for(class);
        self.cycle_count += cost as u64;
        if let Some(profile) = self.profile_session.as_mut() {
            let target_pc = exec_result.new_pc
                .unwrap_or(pc.wrapping_add(exec_result.inst_size as u32));
            profile.dispatch_instruction(pc, target_pc, class, cost);
        }
    }
}
```

Both `run_inner_fast` and `run_inner_logging` call
`self.after_execute(pc, &exec_result)` immediately after
`decode_execute` returns. Replaces the existing inline `cycle_count
+= …` line in both loops.

#### `src/emu/emulator/mod.rs` — cycle-model accessors

```rust
impl Riscv32Emulator {
    pub fn with_cycle_model(mut self, model: CycleModel) -> Self {
        self.cycle_model = model;
        self
    }
    pub fn set_cycle_model(&mut self, model: CycleModel) {
        self.cycle_model = model;
    }
}
```

#### `src/profile/mod.rs` — trait + session updates

`InstClass` re-export from `cycle_model`:
```rust
pub use crate::emu::cycle_model::InstClass;
```
(Replaces the m1-era stub `pub struct InstClass {}`.)

`Collector` trait:
```rust
pub trait Collector: Send {
    fn on_syscall(&mut self, _ctx: &mut EmuCtx, _id: u32, _args: &[u32; 8]) -> SyscallAction { SyscallAction::Forward }
    fn on_perf_event(&mut self, _event: &PerfEvent) {}
    fn on_gate_action(&mut self, _action: GateAction) {}                       // [m2 NEW]
    fn on_instruction(&mut self, _pc: u32, _target_pc: u32,                     // [m2 SIG WIDENED]
                      _class: InstClass, _cycles: u32) {}
    fn finish(&mut self, _ctx: &FinishCtx) -> std::io::Result<()> { Ok(()) }
    fn report_section(&self, _w: &mut dyn std::io::Write) -> std::io::Result<()> { Ok(()) }
}
```

`ProfileSession`:
```rust
impl ProfileSession {
    pub fn dispatch_instruction(&mut self, pc: u32, target_pc: u32,
                                 class: InstClass, cycles: u32) {
        for c in &mut self.collectors {
            c.on_instruction(pc, target_pc, class, cycles);
        }
    }

    /// Extended on_perf_event: fans out to collectors, runs gate,
    /// fans out gate action, propagates Stop.
    pub fn on_perf_event(&mut self, event: PerfEvent) {
        for c in &mut self.collectors { c.on_perf_event(&event); }
        let action = self.gate.as_mut()
            .map(|g| g.evaluate(&event))
            .unwrap_or(GateAction::NoChange);
        for c in &mut self.collectors { c.on_gate_action(action); }       // [m2 NEW]
        if matches!(action, GateAction::Stop) {
            self.pending_halt = Some(HaltReason::ProfileStop);
        }
    }

    /// Called by Riscv32Emulator just before the first instruction runs.
    /// Emits a synthetic profile:start event for events.jsonl + gate boot.
    pub fn start(&mut self) {                                              // [m2 NEW]
        self.on_perf_event(PerfEvent {
            name: EVENT_PROFILE_START,
            kind: PerfEventKind::Instant,
            cycle: 0,
        });
    }

    /// Called by Riscv32Emulator::finish_profile_session before draining.
    pub fn end(&mut self, final_cycle: u64) {                              // [m2 NEW]
        self.on_perf_event(PerfEvent {
            name: EVENT_PROFILE_END,
            kind: PerfEventKind::Instant,
            cycle: final_cycle,
        });
    }
}
```

`AllocCollector` and `EventsCollector` get the default no-op
`on_gate_action` and the widened-signature default no-op
`on_instruction`. No code changes required (defaults handle it).

#### `src/profile/perf_event.rs` — new event constants

```rust
pub const EVENT_PROFILE_START: &str = "profile:start";
pub const EVENT_PROFILE_END:   &str = "profile:end";
```

Added to `KNOWN_EVENT_NAMES` for `EventsCollector`'s validation set.

#### `src/profile/cpu.rs` — new file

```rust
use std::collections::HashMap;
use std::io::{self, Write};
use crate::emu::cycle_model::InstClass;
use super::{Collector, GateAction};

pub struct CpuCollector {
    shadow_stack: Vec<Frame>,
    pub func_stats: HashMap<u32, FuncStats>,
    pub call_edges: HashMap<(u32, u32), CallEdge>,
    active: bool,
    pub total_cycles_attributed: u64,
    pub cycle_model_label: &'static str,
}

#[derive(Clone, Copy)]
struct Frame {
    callee_pc: u32,
    caller_pc: u32,
    cycles_at_entry: u64,
}

#[derive(Default, Clone)]
pub struct FuncStats {
    pub self_cycles: u64,
    pub inclusive_cycles: u64,
    pub calls_in: u64,
    pub calls_out: u64,
}

#[derive(Default, Clone)]
pub struct CallEdge {
    pub count: u64,
    pub inclusive_cycles: u64,
}

const ROOT_PC: u32 = 0;

impl CpuCollector {
    pub fn new(cycle_model_label: &'static str) -> Self {
        Self {
            shadow_stack: Vec::with_capacity(64),
            func_stats: HashMap::new(),
            call_edges: HashMap::new(),
            active: false,                     // R-INIT: gated on by Enable
            total_cycles_attributed: 0,
            cycle_model_label,
        }
    }

    fn current_pc(&self) -> u32 {
        self.shadow_stack.last().map(|f| f.callee_pc).unwrap_or(ROOT_PC)
    }

    fn push_frame(&mut self, caller_pc: u32, callee_pc: u32) {
        self.shadow_stack.push(Frame {
            callee_pc,
            caller_pc,
            cycles_at_entry: self.total_cycles_attributed,
        });
        self.func_stats.entry(callee_pc).or_default().calls_in += 1;
        self.func_stats.entry(caller_pc).or_default().calls_out += 1;
    }

    fn pop_frame(&mut self) {
        let Some(top) = self.shadow_stack.pop() else { return };
        let inclusive = self.total_cycles_attributed - top.cycles_at_entry;
        let stats = self.func_stats.entry(top.callee_pc).or_default();
        stats.inclusive_cycles += inclusive;
        let edge = self.call_edges.entry((top.caller_pc, top.callee_pc)).or_default();
        edge.count += 1;
        edge.inclusive_cycles += inclusive;
    }
}

impl Collector for CpuCollector {
    fn on_gate_action(&mut self, action: GateAction) {
        match action {
            GateAction::Enable  => self.active = true,
            GateAction::Disable => self.active = false,
            _ => {}
        }
    }

    fn on_instruction(&mut self, pc: u32, target_pc: u32, class: InstClass, cycles: u32) {
        if !self.active { return; }
        let stat_pc = self.current_pc();
        self.func_stats.entry(stat_pc).or_default().self_cycles += cycles as u64;
        self.total_cycles_attributed += cycles as u64;
        match class {
            InstClass::JalCall | InstClass::JalrCall => self.push_frame(pc, target_pc),
            InstClass::JalrReturn => self.pop_frame(),
            InstClass::JalTail | InstClass::JalrIndirect => {
                self.pop_frame();
                self.push_frame(pc, target_pc);
            }
            _ => {}
        }
    }

    fn report_section(&self, w: &mut dyn Write) -> io::Result<()> {
        writeln!(w, "=== CPU summary ===")?;
        writeln!(w, "cycle_model={}, total_attributed_cycles={}",
                 self.cycle_model_label, self.total_cycles_attributed)?;
        // top-20 by self_cycles + top-20 by inclusive_cycles
        // (symbolization not available here; emit raw 0xPC)
        Ok(())
    }
}
```

Note on `report_section`: `CpuCollector` does not have access to the
symbol table. It emits raw `0xPC`s. The CLI's
`output::write_report_with_symbols(...)` post-processes the report
file (or emits its own banner) — see CLI changes below for the exact
seam.

#### `src/profile/mod.rs` — gate-impl updates for default-false

Each non-steady-render gate emits `Enable` on `EVENT_PROFILE_START`.
m2 patches m1's gate code in `lp-cli/src/commands/profile/mode/`:

```rust
// mode/compile.rs
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

// mode/startup.rs — Enable on profile:start, Stop on first frame end
// mode/all.rs     — Enable on profile:start, never Stop
// mode/steady_render.rs — UNCHANGED (waits for shader-compile + warmup)
```

#### `src/lib.rs` — re-exports

```rust
#[cfg(feature = "std")]
pub use profile::cpu::CpuCollector;
```

### `lp-cli` (CLI driver)

#### `src/commands/profile/args.rs` — flag updates

```rust
#[derive(clap::Args, Debug)]
pub struct ProfileArgs {
    #[arg(default_value = "examples/basic")]
    pub dir: PathBuf,

    /// Comma-separated collectors: cpu, events, alloc.
    /// Default: cpu (auto-includes events).
    #[arg(long, value_delimiter = ',', default_value = "cpu")]                // [m2: was "events"]
    pub collect: Vec<String>,

    #[arg(long, value_enum, default_value_t = ProfileMode::SteadyRender)]
    pub mode: ProfileMode,

    /// Cycle attribution model.
    #[arg(long, value_enum, default_value_t = CycleModelArg::Esp32C6)]        // [m2 NEW]
    pub cycle_model: CycleModelArg,

    #[arg(long, default_value_t = 200_000_000)]
    pub max_cycles: u64,

    #[arg(long)]
    pub note: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum CycleModelArg { Esp32C6, Uniform }

impl CycleModelArg {
    pub fn label(self) -> &'static str {
        match self { Self::Esp32C6 => "esp32c6", Self::Uniform => "uniform" }
    }
    pub fn to_emu(self) -> CycleModel {
        match self {
            Self::Esp32C6 => CycleModel::Esp32C6,
            Self::Uniform => CycleModel::InstructionCount,
        }
    }
}
```

#### `src/commands/profile/handler.rs` — wiring

Two new chunks:
1. **Cycle model plumbing**:
   `emu = emu.with_cycle_model(args.cycle_model.to_emu());`
2. **`cpu` collector validation + auto-include events**:
   ```rust
   let mut requested: Vec<&str> = args.collect.iter().map(String::as_str).collect();
   if requested.iter().any(|c| *c == "cpu") && !requested.iter().any(|c| *c == "events") {
       requested.push("events");
   }
   for name in &requested {
       match *name {
           "events" => collectors.push(Box::new(EventsCollector::new(...))),
           "alloc"  => collectors.push(Box::new(AllocCollector::new(...))),
           "cpu"    => collectors.push(Box::new(CpuCollector::new(args.cycle_model.label()))),
           other    => bail!("unknown collector: {other}"),
       }
   }
   ```
3. **Output writers at finish**:
   ```rust
   let cpu = downcast_collector::<CpuCollector>(&session)?;
   if let Some(cpu) = cpu {
       let symbols = Symbolizer::new(&trace_symbols);
       output_speedscope::write(cpu, &symbols, &dir.join("cpu-profile.speedscope.json"))?;
       output_cpu_json::write(cpu, &symbols, &dir.join("cpu-profile.json"))?;
   }
   ```

`downcast_collector` helper extracts a `&CpuCollector` from
`session.collectors` by `Any` downcast. Trait bound `Collector:
Any` added in `profile/mod.rs`.

4. **Cycle-model field in `SessionMetadata`** — populate
   `meta.cycle_model = args.cycle_model.label().to_string();`. Field added
   to the m1-era `SessionMetadata` struct.

#### `src/commands/profile/symbolize.rs` — new file

```rust
use std::borrow::Cow;
use lp_riscv_emu::profile::TraceSymbol;

pub struct Symbolizer<'a> {
    /// (lo_addr, hi_addr, name) sorted by lo_addr ascending.
    sorted: Vec<(u32, u32, &'a str)>,
}

impl<'a> Symbolizer<'a> {
    pub fn new(symbols: &'a [TraceSymbol]) -> Self {
        let mut sorted: Vec<_> = symbols.iter()
            .filter(|s| s.size > 0)
            .map(|s| (s.addr, s.addr.saturating_add(s.size), s.name.as_str()))
            .collect();
        sorted.sort_unstable_by_key(|t| t.0);
        Self { sorted }
    }

    pub fn lookup(&self, pc: u32) -> Cow<'a, str> {
        if pc == 0 {
            return Cow::Borrowed("<root>");
        }
        let idx = self.sorted.partition_point(|t| t.0 <= pc).saturating_sub(1);
        if let Some((lo, hi, name)) = self.sorted.get(idx) {
            if pc >= *lo && pc < *hi {
                return Cow::Borrowed(*name);
            }
        }
        if pc >= 0x8000_0000 {
            Cow::Owned(format!("<jit:{:#010x}>", pc))
        } else {
            Cow::Owned(format!("<unknown:{:#010x}>", pc))
        }
    }
}
```

#### `src/commands/profile/output_speedscope.rs` — new file

```rust
//! Speedscope "evented" profile writer.
//!
//! NOTE: events are SYNTHETIC, fabricated from aggregated call_edges
//! at finish time. The flame chart is shape-correct (every callee bar
//! has the right cumulative width relative to its parent) but the
//! x-axis is "synthetic cycles", not wall-clock. Multiple
//! non-contiguous calls to the same function smash together into one
//! bar. For real chronological events, use the cpu-log collector
//! (future work — see m6).
```

Algorithm:
1. Symbolize every PC in `func_stats` and assemble a deduplicated
   `frames: Vec<{ name }>` with a `pc -> frame_idx` map.
2. DFS over `call_edges` keyed by caller, maintaining a running
   `cursor: u64`. For each (caller, callee) in DFS order:
   - Emit `O` event for callee at `cursor`.
   - Recurse into callee's outgoing edges.
   - `cursor += call_edges[(caller, callee)].inclusive_cycles - <recursed inclusive>`
   - Emit `C` event for callee at `cursor`.
3. Wrap in the Speedscope envelope:
   ```json
   {
     "$schema": "https://www.speedscope.app/file-format-schema.json",
     "exporter": "lp-cli profile m2",
     "name": "<dir-name>",
     "activeProfileIndex": 0,
     "profiles": [{
       "type": "evented",
       "name": "<mode>",
       "unit": "none",
       "startValue": 0,
       "endValue": <total_cycles_attributed>,
       "events": [...]
     }],
     "shared": { "frames": [...] }
   }
   ```

Edge case: cycles in `call_edges[(caller, callee)].inclusive_cycles`
that aren't accounted for by recursive children → surfaced as a thin
"self" bar. Edge case: cycles credited to `<root>` (caller_pc = 0) →
form the top-level bars.

#### `src/commands/profile/output_cpu_json.rs` — new file

```json
{
  "schema_version": 1,
  "cycle_model": "esp32c6",
  "total_cycles_attributed": 11000000,
  "func_stats": {
    "0x80001234": {
      "name": "render::frame",
      "self_cycles": 4200000,
      "inclusive_cycles": 8900000,
      "calls_in": 1024,
      "calls_out": 50000
    }
  },
  "call_edges": [
    {
      "caller": "0x80001234",
      "callee": "0x80005678",
      "name": "shader::palette_warm",
      "count": 1024,
      "inclusive_cycles": 4700000
    }
  ]
}
```

PC formatting: `format!("0x{:08x}", pc)`. Symbol names embedded
inline (resolved via `Symbolizer`) to keep this file
self-contained for m3's diff (no `meta.json` cross-reference
needed).

#### `src/commands/profile/output.rs` — report.txt section

Existing `write_report` loops over collectors and calls
`report_section`. m2 piggybacks on this, but `CpuCollector`'s default
`report_section` emits raw PCs. To get symbolized output, the CLI does
a post-pass:

```rust
// In write_report after the per-collector sections:
if let Some(cpu) = downcast::<CpuCollector>(&session) {
    let symbols = Symbolizer::new(&trace_symbols);
    write_cpu_summary_symbolized(&mut report_file, cpu, &symbols)?;
}
```

`write_cpu_summary_symbolized` produces the top-20 self / top-20
inclusive tables with symbol names instead of raw PCs.

(Alternative: pass the `Symbolizer` into `report_section` via a
context arg. Rejected because that requires extending the trait
signature for one collector's needs. The downcast pattern keeps the
trait clean.)

#### `src/commands/profile/mode/compile.rs`, `startup.rs`, `all.rs` — gate updates

Per R-INIT, each non-steady-render gate fires `Enable` on
`EVENT_PROFILE_START`. Pseudocode in the file-level changes section
above. Three small file edits.

Tests (under `mode/<name>.rs#tests`) get a new test case asserting
`Enable` on `profile:start`.

## CLI surface (final m2 shape)

```
lp-cli profile [--dir PROFILES_DIR] EXAMPLE_DIR
  [--collect cpu,events,alloc]                  # default: cpu (events auto-included)
  [--mode steady-render|compile|startup|all]    # default: steady-render
  [--cycle-model esp32c6|uniform]               # default: esp32c6     [m2 NEW]
  [--max-cycles N]                              # default: 200_000_000
  [--note STRING]
```

## Trace directory (final m2 shape)

`profiles/2026-04-19T15-22-01--basic--steady-render/`

| File                            | Source            | Notes                              |
| ------------------------------- | ----------------- | ---------------------------------- |
| `meta.json`                     | handler           | gains `cycle_model` field          |
| `events.jsonl`                  | EventsCollector   | auto-included with cpu             |
| `cpu-profile.json`              | output_cpu_json   | canonical, m3's diff target        |
| `cpu-profile.speedscope.json`   | output_speedscope | browser flame chart                |
| `report.txt`                    | output            | gains `=== CPU summary ===` section|

`heap-trace.jsonl` shows up only when `alloc` is in `--collect`.

## Tests

### Unit tests

- `lp-riscv-emu/src/emu/executor/jump.rs#tests` — handcrafted
  instruction-word fixtures for each new InstClass variant
  (`JalCall`, `JalTail`, `JalrCall`, `JalrReturn`, `JalrIndirect`).
  Asserts both the variant emitted and the `new_pc`/`inst_size`.

- `lp-riscv-emu/src/emu/executor/compressed.rs#tests` — same shape
  for `c.j`, `c.jal`, `c.jr`, `c.jalr` (each with multiple `rs1`
  values where it matters).

- `lp-riscv-emu/src/profile/cpu.rs#tests` — eight scenarios:
  1. `gate_disabled_no_attribution`: events ignored when active=false.
  2. `simple_call_return`: push, attribute, pop. Inclusive cycles
     match wall-clock between push and pop.
  3. `nested_three_deep`: A→B→C→C-return→B-return→A-return.
     Inclusive cycles bubble correctly.
  4. `tail_call_swaps_top`: A→B(tail)→C(tail)→return-from-A. Stack
     never deeper than 1 frame.
  5. `orphaned_return_at_root`: extra return when stack is empty.
     No-op, no panic.
  6. `root_self_cycles`: instructions before any call land under
     `<root>` (PC=0).
  7. `enable_disable_toggle`: gate flips on/off mid-run; cycles only
     attributed during `active=true` windows.
  8. `call_edge_aggregation`: same (caller, callee) called 3 times;
     `call_edges[(caller, callee)].count == 3`.

- `lp-cli/src/commands/profile/symbolize.rs#tests`:
  - hit (PC inside known symbol)
  - miss in RAM → `<jit:0xADDR>`
  - miss elsewhere → `<unknown:0xADDR>`
  - PC == 0 → `<root>`
  - boundary: `pc == addr` (hit), `pc == addr + size - 1` (hit),
    `pc == addr + size` (miss).

- `lp-cli/src/commands/profile/output_speedscope.rs#tests`:
  - 3-frame call graph fixture → output parses as JSON, has correct
    Speedscope shape (`$schema`, `profiles[0].type == "evented"`,
    `events.len() == call_edges.len() * 2`).
  - Sum of all event delta cycles equals `total_cycles_attributed`.

- `lp-cli/src/commands/profile/output_cpu_json.rs#tests`:
  - Round-trip through `serde_json::from_str` / `to_string`.
  - `schema_version == 1` present.
  - Hex format matches `^0x[0-9a-f]{8}$` for all PC keys.

- `lp-cli/src/commands/profile/mode/{compile,startup,all}.rs#tests`:
  - New case: each gate returns `Enable` on `profile:start`.
  - (Existing m1 cases unchanged.)

### Integration tests

- `lp-cli/tests/profile_cpu.rs` (new):
  - `cpu_default_smoke`: `lp-cli profile examples/basic` produces
    `meta.json`, `events.jsonl`, `cpu-profile.json`,
    `cpu-profile.speedscope.json`, `report.txt`. `cpu-profile.json`
    parses; `total_cycles_attributed > 0`; `report.txt` contains
    "CPU summary".
  - `cpu_with_alloc`: `--collect cpu,alloc` produces both
    `cpu-profile.json` and `heap-trace.jsonl`.
  - `cpu_uniform_model`: `--cycle-model uniform` →
    `meta.json.cycle_model == "uniform"` and all instruction costs
    are 1 (sanity check on a tiny example).
  - `cpu_compile_mode`: `--mode compile` produces a non-empty
    flame chart (smoke test that gate Enable-on-profile-start works).

## Out of scope

- m3 — `--diff [PATH]` and `lp-cli profile diff`.
- m4 — `HardwarePerfSink`, esp32c6 cycle-cost refinement.
- m5 — JIT symbol overlay (`lpvm-native` reports JIT'd entries to host).
- m6 — `cpu-log` collector for chronological event streaming
  (the "detailed view" — separate JSONL file, gigabytes for a
  multi-frame capture, parsed by a separate writer).
- Per-event `arg: u32` payload (ABI room reserved in m1, no consumer).
- `--raw-events` opt-in.
- Folded-stack output, callgrind text format.

## Migration risk

- **Decoder changes** are pure refactoring of classification logic;
  semantics unchanged. Existing instruction-execution tests remain
  green.
- **`Collector` trait widening** is backwards-compatible — defaults
  cover the no-op case for existing collectors.
- **`--collect` default flip** (events → cpu) is user-visible.
  Documented in m2's CLI section. Anyone relying on m1's default
  behavior will see new files appear; nothing existing breaks.
- **Synthetic `profile:start`/`profile:end`** events are additive in
  `events.jsonl`. The non-steady-render gates change behavior:
  previously they captured nothing meaningful (m1 logged
  Enable/Disable but didn't gate); now they capture from boot to
  their natural Stop trigger. Documented explicitly.
- **`SessionMetadata.cycle_model` field** is additive on
  `meta.json`. m1 Q9 policy: schema_version stays 1; nothing
  consumes meta.json yet.
