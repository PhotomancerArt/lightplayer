# Phase 1 — Profile module skeleton + Collector trait

Create the new `lp-riscv-emu/src/profile/` module with the
`Collector` trait, `ProfileSession` shell, and supporting types.
**No integration with `Riscv32Emulator` yet** — that comes in
phase 4. This phase produces a self-contained module that compiles
on its own, with one no-op test collector to prove the trait shape.

This phase can run in parallel with phase 2 (Cargo feature rename);
they touch disjoint files.

## Subagent assignment

`generalPurpose` subagent. Tightly scoped: new module only,
mechanical translation from design doc to code.

## Files to create

```
lp-riscv/lp-riscv-emu/src/
├── profile/
│   ├── mod.rs       # NEW
│   └── alloc.rs     # NEW (placeholder; phase 3 fills it)
└── lib.rs           # UPDATE: add `pub mod profile;`
```

## Module contents

### `profile/mod.rs`

Define everything per the design doc's "`Collector` trait" and
"`ProfileSession`" sections:

- `pub trait Collector: Send` with all methods + default impls.
- `pub struct ProfileSession { trace_dir, collectors }` with
  `new`, `dispatch_syscall`, `finish`.
- `pub struct EmuCtx<'a>` with `pc`, `regs`, `cycle_count`,
  `instruction_count`, `memory: &'a Memory`, plus
  `unwind_backtrace(&self) -> Vec<u32>` (port from
  `alloc_trace.rs`'s existing logic, kept here so collectors
  can call it; do NOT delete the original yet — phase 6
  removes `alloc_trace.rs`).
- `pub enum SyscallAction { Pass, Handled, Halt(HaltReason) }`.
- `pub enum HaltReason { Oom { size: u32 } }`.
- `pub struct FinishCtx<'a> { pub trace_dir: &'a Path }`.
- `pub struct PerfEvent {}` — empty struct, doc-comment "m1
  fills in".
- `pub enum InstClass {}` — empty enum is fine for m0; m1 fills.
  (If `enum InstClass {}` is awkward as a method param, use
  `pub struct InstClass {}` instead — pick whichever lets the
  default `on_instruction` impl compile.)
- `pub struct SessionMetadata` per design doc.
- `pub struct TraceSymbol` (port from existing
  `alloc_trace::TraceSymbol` — `name: String, addr: u32, size: u32`,
  serde-serializable).

### `profile/alloc.rs` (placeholder)

```rust
//! Allocation collector. Implementation lands in phase 3.
```

A bare module file so phase 1 can declare `pub mod alloc;` in
`profile/mod.rs` without breaking the build.

### `lib.rs` update

Add `pub mod profile;` next to existing module declarations.
Do NOT touch `pub mod alloc_trace;` — it stays alive through
phase 5.

## ProfileSession::finish details

Per design doc:
1. For each collector: call `collector.finish(&FinishCtx { trace_dir: &self.trace_dir })`, propagate any io::Error.
2. Build a `String` buffer; for each collector emit
   `"=== {} ===\n"` with `report_title()`, then call
   `report_section(&mut buf)`, then append `"\n"`.
3. `print!("{}", buf)` to stdout.
4. Write `buf` to `<trace_dir>/report.txt`.
5. Return aggregate event count. Since collectors don't expose
   counts via the trait, return `0` for m0 and add a TODO. (Phase 4
   refines this — the existing `finish_alloc_trace` returned the
   alloc event count and the CLI logged it; we'll preserve the
   logging by having `AllocCollector` expose its own getter that
   the handler reads directly. The trait stays clean.)

## ProfileSession::new details

1. `std::fs::create_dir_all(&trace_dir)?`.
2. Build the meta.json `serde_json::Value`:

   ```json
   {
     "schema_version": 1,
     "timestamp": "...",
     "project": "...",
     "workload": "...",
     "note": null,
     "clock_source": "emu_estimated",
     "frames_requested": 10,
     "symbols": [...],
     "collectors": {
       "<collector.name()>": <collector.meta_json()>,
       ...
     }
   }
   ```

3. Write to `<trace_dir>/meta.json` pretty-printed (`to_writer_pretty`).
4. Construct and return `Self`.

## Validation

```bash
cargo check -p lp-riscv-emu
cargo build -p lp-riscv-emu
```

Add a tiny unit test inside `profile/mod.rs` with a no-op
collector to prove the trait compiles end-to-end:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    struct NoopCollector;
    impl Collector for NoopCollector {
        fn name(&self) -> &'static str { "noop" }
        fn finish(&mut self, _: &FinishCtx) -> std::io::Result<()> { Ok(()) }
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
            frames_requested: 0,
            symbols: vec![],
        };
        let mut session = ProfileSession::new(
            tmp.path().to_path_buf(),
            &metadata,
            vec![Box::new(NoopCollector)],
        ).unwrap();
        assert!(tmp.path().join("meta.json").exists());
        session.finish().unwrap();
        assert!(tmp.path().join("report.txt").exists());
    }
}
```

(`tempfile` is already a dev-dep in this crate; if not, add it.)

```bash
cargo test -p lp-riscv-emu profile::
```

## Out of scope for this phase

- Anything in `Riscv32Emulator` (phase 4).
- AllocCollector body (phase 3).
- CLI changes (phase 5).
- Cargo feature rename (phase 2, parallel).
