# CPU Profile m0 — Foundation Refactor — Design

This implements **m0** of the CPU profile roadmap
(`docs/roadmaps/2026-04-19-cpu-profile/m0-foundation.md`). m0 is pure
restructuring: introduces the unified profiling infrastructure
(`Collector` trait, `ProfileSession`, unified profile dir layout),
ports the existing alloc-trace into it, and ships `lp-cli profile
--collect alloc` at functional parity with the now-removed
`lp-cli mem-profile` and `lp-cli heap-summary` commands.

**No new functionality.** The diff is module reorganization, command
surface change, and a new collector abstraction sized for m1+ growth.

## Scope of work

In scope:

- New module `lp-riscv-emu/src/profile/` with `Collector` trait,
  `ProfileSession`, supporting types (`EmuCtx`, `SyscallAction`,
  `HaltReason`, `FinishCtx`, `PerfEvent` stub, `SessionMetadata`).
- New file `lp-riscv-emu/src/profile/alloc.rs` containing
  `AllocCollector` implementing `Collector`. Absorbs both today's
  `AllocTracer` (in `alloc_trace.rs`) and today's heap-summary
  formatting code (in `lp-cli/src/commands/heap_summary/`).
- Replace `Riscv32Emulator::alloc_tracer` field with
  `profile_session`. Builder/finalizer methods renamed accordingly.
- `run_loops::handle_syscall` SYSCALL_ALLOC_TRACE path: dispatch
  through `profile_session`, no direct field access.
- New `lp-cli profile` command with two subcommands (`profile run`
  via positional dir; `profile diff` stub). `--collect` is comma-
  separated, defaults to `alloc`. Profile dir layout:
  `profiles/<timestamp>--<workload>[--<note>]/` (note: `profiles/`,
  not legacy `traces/`).
- Rename Cargo feature `alloc-trace` → `profile` in `fw-emu` and
  `lp-riscv-emu-guest`. Update `cfg(feature = "alloc-trace")` gates.
  Syscall constant names (`SYSCALL_ALLOC_TRACE`, etc.) are kept.
- Delete `mem_profile/`, `heap_summary/`, `alloc_trace.rs`.
- Rename `examples/mem-profile/` → `examples/perf/baseline/`.
- Update `justfile` (drop two recipes, add one).
- Rewire existing `fw-tests/tests/alloc_trace_emu.rs` →
  `profile_alloc_emu.rs`.
- New CLI smoke test: `lp-cli/tests/profile_alloc_smoke.rs`.

Out of scope (deferred to later milestones per roadmap):

- CPU collector, events collector, perf-event syscall (m1).
- `--mode` flag, `ProfileMode` enum (m1).
- Functional `profile diff` (m2 — stub only here).
- `--diff [PATH]` flag on `profile` (m2).
- Hardware sink, console parser, correlation (m3).
- JIT symbol overlay (m4).
- `docs/design/native/fw-profile/` documentation (m5).
- Any change to alloc data model or wire format beyond what the
  refactor mechanically requires.

## File structure

```
lp-riscv/lp-riscv-emu/src/
├── profile/
│   ├── mod.rs                 # NEW: trait + session + supporting types
│   └── alloc.rs               # NEW: AllocCollector + report code
├── alloc_trace.rs             # DELETED
├── lib.rs                     # UPDATE: profile module export
└── emu/emulator/
    ├── state.rs               # UPDATE: alloc_tracer field → profile_session
    └── run_loops.rs           # UPDATE: SYSCALL_ALLOC_TRACE dispatched via session

lp-riscv/lp-riscv-emu-guest/
├── Cargo.toml                 # UPDATE: feature alloc-trace → profile
└── src/
    ├── allocator.rs           # UPDATE: cfg gates renamed
    └── syscall.rs             # UPDATE: cfg gate renamed

lp-fw/fw-emu/Cargo.toml        # UPDATE: feature alloc-trace → profile

lp-fw/fw-tests/tests/
└── profile_alloc_emu.rs       # RENAMED from alloc_trace_emu.rs; rewired

lp-cli/src/
├── main.rs                    # UPDATE: register profile, drop two old cmds
└── commands/
    ├── mod.rs                 # UPDATE: profile module
    ├── profile/
    │   ├── mod.rs             # NEW: re-exports
    │   ├── args.rs            # NEW: ProfileArgs, ProfileDiffArgs
    │   ├── handler.rs         # NEW: run handler (moves mem_profile logic)
    │   └── diff_stub.rs       # NEW: m0 stub
    ├── mem_profile/           # DELETED
    └── heap_summary/          # DELETED

lp-cli/tests/
└── profile_alloc_smoke.rs     # NEW

examples/
└── perf/baseline/             # RENAMED from examples/mem-profile/

justfile                       # UPDATE: drop mem-profile / heap-summary,
                               #         add profile recipe
```

## Conceptual architecture

### Component overview

```
lp-riscv-emu                                                lp-cli
─────────────                                               ──────
                                                            commands/profile/
profile/mod.rs                                                  args.rs
   ├── trait Collector                                          handler.rs ─────┐
   ├── struct ProfileSession                                    diff_stub.rs    │
   ├── struct SessionMetadata (top-level meta.json fields)                      │
   ├── struct EmuCtx<'_>                                                        │
   ├── enum SyscallAction { Pass, Handled, Halt(HaltReason) }                   │
   ├── enum HaltReason { Oom { size } }                                         │
   ├── struct FinishCtx<'_> { trace_dir }                                       │
   └── struct PerfEvent (m0 stub; m1 fills in)                                  │
                                                                                │
profile/alloc.rs                                                                │
   ├── struct AllocCollector ─────── impl Collector                             │
   ├── struct AllocEvent (wire format, unchanged shape)                         │
   ├── struct LiveAllocation, OomEvent, RunningStats (moved from heap_summary)  │
   ├── struct SymbolResolver (moved from heap_summary/resolver.rs)              │
   └── struct AllocReport (moved from heap_summary/report.rs)                   │
                                                                                │
emu/emulator/state.rs                                                           │
   └── Riscv32Emulator { profile_session: Option<ProfileSession>, ... }         │
       ├── with_profile_session(SessionConfig) → Result<Self>                   │
       └── finish_profile_session() → io::Result<u64>                           │
                                                                                │
emu/emulator/run_loops.rs                                                       │
   └── handle_syscall(...) ── SYSCALL_ALLOC_TRACE:                              │
         build EmuCtx, dispatch through profile_session                  ◄──────┘
         match action { Pass | Handled | Halt(Oom) → StepResult::Oom }
```

### `Collector` trait

```rust
pub trait Collector: Send {
    fn name(&self) -> &'static str;
    fn report_title(&self) -> &'static str { self.name() }

    fn meta_json(&self) -> serde_json::Value { serde_json::json!({}) }

    fn on_syscall(
        &mut self,
        _ctx: &mut EmuCtx<'_>,
        _id: u32,
        _args: &[u32],
    ) -> SyscallAction { SyscallAction::Pass }

    fn on_instruction(&mut self, _pc: u32, _kind: InstClass, _cycles: u32) {}

    fn on_perf_event(&mut self, _evt: &PerfEvent) {}

    fn finish(&mut self, ctx: &FinishCtx) -> std::io::Result<()>;

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result;
}
```

Default impls cover everything except `name`, `finish`,
`report_section` — the bare minimum for a no-op collector. m0's
`AllocCollector` overrides `report_title`, `meta_json`, `on_syscall`,
`finish`, `report_section`.

`EmuCtx<'_>` exposes the read-only emulator surface a collector needs:

```rust
pub struct EmuCtx<'a> {
    pub pc: u32,
    pub regs: &'a [i32; 32],
    pub cycle_count: u64,
    pub instruction_count: u64,
    pub memory: &'a Memory,
}

impl<'a> EmuCtx<'a> {
    pub fn unwind_backtrace(&self) -> Vec<u32> { /* same as today */ }
}
```

`SyscallAction` semantics:

| Variant | Effect |
|---------|--------|
| `Pass` | Collector didn't claim it; emulator runs normal path. |
| `Handled` | Collector consumed it; emulator sets `regs[A0] = 0` and continues. |
| `Halt(HaltReason::Oom { size })` | Run loop returns `StepResult::Oom { OomInfo { size, pc } }`. |

`SessionMetadata` carries the top-level meta.json fields:

```rust
pub struct SessionMetadata {
    pub schema_version: u32,        // = 1
    pub timestamp: String,          // RFC3339 or sortable form
    pub project: String,            // project uid
    pub workload: String,           // raw `DIR` arg
    pub note: Option<String>,       // raw `--note` arg
    pub clock_source: &'static str, // "emu_estimated" in m0
    pub frames_requested: u32,      // m0 only; m1 replaces with `mode`
    pub symbols: Vec<TraceSymbol>,
}
```

### `ProfileSession`

```rust
pub struct ProfileSession {
    trace_dir: PathBuf,
    collectors: Vec<Box<dyn Collector>>,
}

impl ProfileSession {
    pub fn new(
        trace_dir: PathBuf,
        metadata: &SessionMetadata,
        collectors: Vec<Box<dyn Collector>>,
    ) -> std::io::Result<Self> {
        // 1. Create trace_dir if missing
        // 2. Assemble JSON: top-level fields + `collectors: { <name>: meta_json() }`
        // 3. Write meta.json (pretty)
        Ok(Self { trace_dir, collectors })
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

    pub fn finish(&mut self) -> std::io::Result<u64> {
        // 1. For each collector: collector.finish(&FinishCtx { trace_dir })
        // 2. Build report.txt buffer:
        //    for each collector: emit "=== <report_title> ===\n",
        //                        call report_section, append "\n"
        // 3. Print buffer to stdout, write to report.txt
        // 4. Return aggregate event count (sum of collector-reported counts)
    }
}
```

### `Riscv32Emulator` integration

The current `alloc_tracer: Option<AllocTracer>` field is replaced
with `profile_session: Option<ProfileSession>`. The `Riscv32Emulator`
keeps the existing two-method shape (`with_*` builder + `finish_*`
finalizer) renamed:

```rust
impl Riscv32Emulator {
    pub fn with_profile_session(
        mut self,
        trace_dir: PathBuf,
        metadata: &SessionMetadata,
        collectors: Vec<Box<dyn Collector>>,
    ) -> std::io::Result<Self> {
        self.profile_session = Some(ProfileSession::new(trace_dir, metadata, collectors)?);
        Ok(self)
    }

    pub fn finish_profile_session(&mut self) -> std::io::Result<u64> {
        match self.profile_session.as_mut() {
            Some(s) => s.finish(),
            None => Ok(0),
        }
    }
}
```

### Run loop dispatch

`run_loops::handle_syscall` for `SYSCALL_ALLOC_TRACE`:

```rust
SYSCALL_ALLOC_TRACE => {
    if let Some(session) = self.profile_session.as_mut() {
        // Borrow split: build the ctx from emulator fields,
        // then dispatch on the (already-borrowed) session.
        let mut ctx = EmuCtx {
            pc: self.pc,
            regs: &self.regs,
            cycle_count: self.cycle_count,
            instruction_count: self.instruction_count,
            memory: &self.memory,
        };
        let action = session.dispatch_syscall(
            &mut ctx,
            SYSCALL_ALLOC_TRACE,
            &syscall_info.args.map(|a| a as u32),
        );
        match action {
            SyscallAction::Pass => { /* fall through to default */ }
            SyscallAction::Handled => {
                self.regs[Gpr::A0.num() as usize] = 0;
                return Ok(StepResult::Continue);
            }
            SyscallAction::Halt(HaltReason::Oom { size }) => {
                return Ok(StepResult::Oom(OomInfo { size, pc: self.pc }));
            }
        }
    }
    self.regs[Gpr::A0.num() as usize] = 0;
    Ok(StepResult::Continue)
}
```

(Practical note: `&self.profile_session` and `&self.memory` overlap
in borrow lifetimes but are separate fields, so `self.split_borrow`
or destructuring will work; the executor team has done similar splits
already. If the borrow proves awkward, the alternative is having
`dispatch_syscall` take ownership of the action build via a small
helper that captures the needed fields up front.)

### `AllocCollector`

```rust
pub struct AllocCollector {
    writer: BufWriter<File>,
    event_count: u64,
    heap_start: u32,
    heap_size: u32,
}

impl AllocCollector {
    pub fn new(trace_dir: &Path, heap_start: u32, heap_size: u32) -> io::Result<Self> {
        let path = trace_dir.join("heap-trace.jsonl");
        let writer = BufWriter::new(File::create(&path)?);
        Ok(Self { writer, event_count: 0, heap_start, heap_size })
    }
}

impl Collector for AllocCollector {
    fn name(&self) -> &'static str { "alloc" }
    fn report_title(&self) -> &'static str { "Heap Allocation" }

    fn meta_json(&self) -> serde_json::Value {
        serde_json::json!({
            "heap_start": self.heap_start,
            "heap_size": self.heap_size,
        })
    }

    fn on_syscall(
        &mut self,
        ctx: &mut EmuCtx<'_>,
        id: u32,
        args: &[u32],
    ) -> SyscallAction {
        if id != SYSCALL_ALLOC_TRACE { return SyscallAction::Pass; }
        let event_type = args[0];
        let frames = ctx.unwind_backtrace();
        let ic = ctx.instruction_count;

        match event_type {
            ALLOC_TRACE_ALLOC => {
                self.write(AllocEvent { t: "A", ptr: args[1], sz: args[2], ic, frames, free: args[3], old_ptr: None, old_sz: None });
                SyscallAction::Handled
            }
            ALLOC_TRACE_DEALLOC => { /* analogous */ SyscallAction::Handled }
            ALLOC_TRACE_REALLOC => { /* analogous */ SyscallAction::Handled }
            ALLOC_TRACE_OOM => {
                let size = args[2];
                self.write(AllocEvent { t: "O", ptr: 0, sz: size, ic, frames, free: 0, old_ptr: None, old_sz: None });
                SyscallAction::Halt(HaltReason::Oom { size })
            }
            _ => SyscallAction::Handled,
        }
    }

    fn finish(&mut self, _ctx: &FinishCtx) -> io::Result<()> {
        self.writer.flush()
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        // Re-uses moved heap-summary code (handler.rs + report.rs).
        // Reads back its own heap-trace.jsonl + parent meta.json,
        // builds the AllocReport, writes to `w`.
        // (Stream + format split lives entirely inside profile/alloc.rs.)
    }
}
```

The "moved heap-summary code" includes `RunningStats`,
`LiveAllocation`, `OomEvent`, `AllocReport`, `SymbolResolver`, and
their helpers (`fmt_num`, `find_matching_close`, `last_path_component`,
etc.) — all currently in `lp-cli/src/commands/heap_summary/{handler,
report,resolver}.rs`. They become private items in
`profile/alloc.rs`.

### `lp-cli profile` command

```
lp-cli profile [DIR=examples/basic]
               [--collect <list>=alloc]   # comma-separated; m0 validates {alloc} only
               [--frames N=10]            # m0 temp; m1 replaces with --mode
               [--note STR]

lp-cli profile diff <a> <b>               # stub: exit 2 + stderr message
```

`commands/profile/handler.rs` is largely a port of today's
`mem_profile/handler.rs`:
- Build fw-emu with `--features profile` (renamed feature).
- Load ELF, extract `symbol_list`.
- Build `SessionMetadata` from CLI args + ELF metadata.
- Construct collectors per `--collect` list. m0: just
  `AllocCollector::new(&trace_dir, heap_start, heap_size)`.
- Construct `Riscv32Emulator::new(...).with_profile_session(...)`.
- Wire up transport, drive workload (`advance_time(40)` × frames).
- `finish_profile_session()` (which writes `report.txt` + prints).
- Print final trace dir path.

`diff_stub.rs`:

```rust
pub fn handle_profile_diff(args: ProfileDiffArgs) -> ! {
    eprintln!("error: 'lp-cli profile diff' is not yet implemented (planned for cpu-profile m2)");
    eprintln!("trace dirs: {}, {}", args.a.display(), args.b.display());
    std::process::exit(2);
}
```

### Feature gate rename

```
cfg(feature = "alloc-trace") → cfg(feature = "profile")
```

In:
- `lp-fw/fw-emu/Cargo.toml` (key + value `lp-riscv-emu-guest/profile`)
- `lp-riscv/lp-riscv-emu-guest/Cargo.toml` (key only)
- `lp-riscv/lp-riscv-emu-guest/src/allocator.rs` (5 gates)
- `lp-riscv/lp-riscv-emu-guest/src/syscall.rs` (1 gate)
- `lp-cli/src/commands/profile/handler.rs` build invocation
- `lp-fw/fw-tests/tests/profile_alloc_emu.rs` build invocation

Syscall constant names (`SYSCALL_ALLOC_TRACE`, `ALLOC_TRACE_*`) and
numeric values are unchanged.

## Validation

```bash
# Existing alloc-trace tests still pass through new infrastructure
cargo test -p lp-riscv-emu

# Renamed end-to-end emu test
cargo test -p fw-tests --test profile_alloc_emu

# New CLI smoke test
cargo test -p lp-cli --test profile_alloc_smoke

# Manual sanity: new command produces expected dir contents
cargo run -p lp-cli -- profile examples/basic --collect alloc --frames 2 --note manual
ls profiles/*--examples-basic--manual/
#   meta.json  heap-trace.jsonl  report.txt

# Diff stub exits non-zero with informative stderr
cargo run -p lp-cli -- profile diff /tmp/a /tmp/b ; echo "exit=$?"
#   → "exit=2", stderr message visible

# fw-esp32 still builds with renamed feature
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server

# Host validation across the workspace excluding RV32-only crates
just build-host
```

## Estimated scope

- New code: ~400 LOC for trait + session + alloc collector skeleton.
- Moved code: ~600 LOC (alloc_trace.rs body, mem_profile/handler.rs
  body, heap_summary/{handler,report,resolver}.rs bodies).
- Net diff likely +200 to +400 LOC after dedup.
- Files touched: ~15-20.
- No algorithmic complexity. Restructuring + new command surface.
