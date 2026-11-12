# Phase 3 — Port `AllocCollector` + move heap-summary report code

Build the real `AllocCollector` inside `profile/alloc.rs` by
moving body code from two existing locations:

1. `lp-riscv-emu/src/alloc_trace.rs` — the streaming/wire-format
   side (`AllocTracer`, `AllocEvent`, file writer, OOM handling).
2. `lp-cli/src/commands/heap_summary/{handler,report,resolver}.rs`
   — the offline analysis/report side (`AllocReport`,
   `LiveAllocation`, `OomEvent`, `RunningStats`,
   `SymbolResolver`, format helpers).

Both halves end up as private items inside `profile/alloc.rs`.
The original files **stay in place and keep working** — they get
deleted in phase 6 only after the new path is fully wired
(phases 4–5) and tested.

Depends on phase 1 (skeleton must exist). Independent of phase 2.

## Subagent assignment

`generalPurpose` subagent. The largest mechanical move in m0;
sized for a single agent, no parallelism gain from splitting.

## What to copy from `alloc_trace.rs`

Inline the relevant items into `profile/alloc.rs` (do **not**
re-export from the old module):

- `AllocEvent` struct (wire format) — the JSON-serializable
  shape with fields `t, ptr, sz, ic, frames, free, old_ptr,
  old_sz`. Keep field names byte-for-byte: heap-trace.jsonl
  must match what the existing tests + offline scripts expect.
- The constants `ALLOC_TRACE_ALLOC`, `ALLOC_TRACE_DEALLOC`,
  `ALLOC_TRACE_REALLOC`, `ALLOC_TRACE_OOM`, and
  `SYSCALL_ALLOC_TRACE` — re-import from
  `lp-riscv-emu-guest::syscall` (or whatever the canonical
  source is); do NOT redefine.
- The `unwind_backtrace` body — already moved to `EmuCtx`
  during phase 1, so `AllocCollector` just calls
  `ctx.unwind_backtrace()`.
- Whatever `BufWriter`/`File` plumbing `AllocTracer` does for
  `heap-trace.jsonl`.

## What to copy from `lp-cli/src/commands/heap_summary/`

Move the bodies of these into `profile/alloc.rs` as private items:

- From `handler.rs`: the offline parsing loop (read meta.json,
  iterate heap-trace.jsonl, fold events into `RunningStats` +
  `LiveAllocation` map + `OomEvent` list).
- From `report.rs`: `AllocReport`, formatting code,
  `fmt_num`, `find_matching_close`, `last_path_component`,
  any other helpers.
- From `resolver.rs`: `SymbolResolver` (uses `rustc_demangle`).
  Add `rustc_demangle` to `lp-riscv-emu`'s dependencies; remove
  it from `lp-cli` only in phase 6 (when `heap_summary/` is
  actually deleted).

Keep all of these `pub(crate)` or private — nothing in this
collector needs to be visible outside `profile/alloc.rs`.

## `AllocCollector` shape

Per design doc:

```rust
pub struct AllocCollector {
    writer: BufWriter<File>,
    event_count: u64,
    heap_start: u32,
    heap_size: u32,
    trace_path: PathBuf,  // remember for finish-time report read-back
}

impl AllocCollector {
    pub fn new(trace_dir: &Path, heap_start: u32, heap_size: u32) -> io::Result<Self> { ... }

    pub fn event_count(&self) -> u64 { self.event_count }   // for CLI logging

    fn write(&mut self, evt: AllocEvent) -> io::Result<()> { ... }
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

    fn on_syscall(&mut self, ctx: &mut EmuCtx<'_>, id: u32, args: &[u32]) -> SyscallAction {
        // dispatch on event_type per design doc table
    }

    fn finish(&mut self, _ctx: &FinishCtx) -> io::Result<()> {
        self.writer.flush()
    }

    fn report_section(&self, w: &mut dyn fmt::Write) -> fmt::Result {
        // 1. Re-open self.trace_path for read.
        // 2. Read sibling meta.json (parent of self.trace_path) to get
        //    `symbols` (top-level) and `collectors.alloc.heap_start/size`.
        // 3. Build AllocReport using the moved code.
        // 4. Write to `w` *without* a top-level header — ProfileSession
        //    prints the "=== Heap Allocation ===" banner.
        Ok(())
    }
}
```

Note on report_section reading meta.json: this is a small
inefficiency (we already have heap_start/heap_size as fields),
but the symbols list isn't held in `AllocCollector` and pulling
it from meta.json keeps the collector decoupled from
`SessionMetadata`. Acceptable for m0; revisit if needed.

## Steps

1. Open `profile/alloc.rs` (placeholder from phase 1).
2. Add module-level imports and definitions.
3. Copy `AllocEvent` from `alloc_trace.rs`. Keep serde derives.
4. Implement `AllocCollector::new`, `write`, `event_count`.
5. Implement `Collector` trait — start with stubs returning
   `SyscallAction::Pass` and empty `report_section`, get it
   compiling.
6. Fill in `on_syscall` per design table (alloc/dealloc/realloc
   write events; OOM writes event then returns `Halt(Oom)`).
7. Move heap_summary bodies as private items.
8. Implement `report_section` using the moved code.

## Validation

```bash
cargo check -p lp-riscv-emu
cargo build -p lp-riscv-emu

# alloc_trace.rs is still in the build but unused by the new
# collector — should still compile.
cargo test -p lp-riscv-emu
```

The actual end-to-end verification (alloc collector produces
a valid heap-trace.jsonl + report.txt) happens in phase 7 after
the run loop and CLI are wired.

## Out of scope for this phase

- Replacing `AllocTracer` in `Riscv32Emulator` (phase 4).
- CLI changes (phase 5).
- Deleting `alloc_trace.rs` and `heap_summary/` (phase 6).
