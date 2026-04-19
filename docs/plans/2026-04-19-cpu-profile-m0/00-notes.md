# CPU Profile m0 — Foundation Refactor — Notes

This plan implements **m0** of the CPU profile roadmap
(`docs/roadmaps/2026-04-19-cpu-profile/`). m0 is pure restructuring: it
introduces the `Collector` trait, `ProfileSession`, the unified trace
dir layout, and a new `lp-cli profile` command, then ports the existing
`alloc-trace` machinery into that shape. **No new functionality**;
existing alloc-trace wire format is preserved byte-for-byte.

## Scope of work

In scope (per `docs/roadmaps/2026-04-19-cpu-profile/m0-foundation.md`):

- New module `lp-riscv-emu/src/profile/` with:
  - `mod.rs` — `Collector` trait, `ProfileSession`, `FinishCtx`,
    `SyscallAction`. Trait shape designed once with m1's
    `CpuCollector` / `EventsCollector` already in mind
    (`on_instruction`, `on_perf_event` exist as no-op defaults).
  - `alloc.rs` — `AllocCollector` implementing `Collector`. Same data
    shape, same wire format, same on-disk filenames as today's
    `AllocTracer`. Owns the `report_section` formatting that today
    lives in `lp-cli/src/commands/heap_summary/`.
- Delete `lp-riscv-emu/src/alloc_trace.rs`.
- Wire `ProfileSession` into `Riscv32Emulator` (replace
  `Option<AllocTracer>` with `Option<ProfileSession>`); update
  `with_alloc_trace`/`finish_alloc_trace` → `with_profile_session`/
  `finish_profile_session`; update `run_loops.rs` syscall dispatch.
- New `lp-cli profile` command in `lp-cli/src/commands/profile/` with:
  - `lp-cli profile [DIR=examples/basic] [--collect alloc] [--frames N=10] [--note STR]`
  - `lp-cli profile diff <a> <b>` stub that prints "implemented in m2".
  - `--collect` parser is forward-compatible (m1 will add `cpu`,
    `events`, etc.). m0 only validates `alloc`.
- Combined `report.txt` written by `ProfileSession::finish()`. Iterates
  enabled collectors calling `report_section()`; concatenates with
  section dividers; printed to stdout AND written to disk.
- Delete `lp-cli/src/commands/mem_profile/` and
  `lp-cli/src/commands/heap_summary/`. Remove their CLI registrations
  in `lp-cli/src/main.rs`. Move heap-summary's formatting code into
  `AllocCollector::report_section`.
- Rename Cargo feature `alloc-trace` → `profile` in
  `lp-fw/fw-emu/Cargo.toml` and `lp-riscv-emu-guest/Cargo.toml`. Update
  `cfg(feature = "alloc-trace")` gates accordingly.
- Update existing `fw-tests/tests/alloc_trace_emu.rs` (and
  `lp-riscv-emu` unit tests, if any) to construct `ProfileSession`
  with `AllocCollector` instead of `AllocTracer`.
- Add a CLI smoke test for `lp-cli profile --collect alloc`.
- Update `justfile` recipes (`mem-profile`, `heap-summary` → `profile`).

Out of scope (per roadmap):

- CPU collector, events collector, perf-event syscalls (m1).
- `--mode` flag and `ProfileMode` enum (m1).
- Functional `profile diff` (m2 — stub only).
- `--diff [PATH]` flag on `profile` (m2).
- Hardware sink, console parser, correlation (m3).
- JIT symbol overlay (m4).
- Documentation under `docs/design/native/fw-profile/` (m5).
- Any change to `AllocCollector`'s data model or output format.

## Current state of the codebase

### `lp-riscv-emu/src/alloc_trace.rs` (~99 LOC)

Defines `TraceMetadata`, `TraceSymbol`, `AllocEvent`, and
`AllocTracer`. `AllocTracer` writes `meta.json` (pretty-printed JSON)
and opens `heap-trace.jsonl` for append; `record_event` serializes an
event per line; `finish` flushes and returns the event count.
Std-only (`#[cfg(feature = "std")]` gated at module level in
`lib.rs`).

### `lp-riscv-emu/src/emu/emulator/state.rs`

`Riscv32Emulator` has a `#[cfg(feature = "std")] alloc_tracer:
Option<AllocTracer>` field. Two builder methods:
- `with_alloc_trace(trace_dir, metadata)` — constructs the tracer.
- `finish_alloc_trace()` — flushes, returns event count.

### `lp-riscv-emu/src/emu/emulator/run_loops.rs`

`handle_syscall` directly handles `SYSCALL_ALLOC_TRACE` by reading the
syscall args, calling `unwind_backtrace`, building an `AllocEvent`,
and calling `self.alloc_tracer.as_mut().unwrap().record_event(&event)`
when the tracer is enabled (~70 LOC of inline handling for A/D/R/O
event variants). OOM returns `StepResult::Oom`.

### `lp-cli/src/commands/mem_profile/` (~250 LOC)

`handler.rs`:
- Builds fw-emu with feature `alloc-trace` + frame pointers.
- Loads ELF via `lp_riscv_elf::load_elf`.
- Constructs trace dir name `traces/<timestamp>--<dir>--<note>/` via
  local `kebab_case` helper. Format already matches roadmap.
- Constructs `TraceMetadata` from ELF symbol list.
- Builds `Riscv32Emulator` with `with_alloc_trace`.
- Sets up `LpClient` with `SerialEmuClientTransport`, ticks frames
  via `advance_time(40)`, then stops projects.
- Calls `finish_alloc_trace`, then calls
  `crate::commands::heap_summary::analyze_trace_dir` to produce the
  report, prints to stdout, writes to `report.txt`.

`mod.rs` and `args.rs` are tiny (one-line re-exports + `MemProfileArgs`
struct).

### `lp-cli/src/commands/heap_summary/` (~700 LOC)

- `args.rs` — `HeapSummaryArgs { trace_dir, top }`.
- `mod.rs` — re-exports.
- `handler.rs` — single-pass stream over `heap-trace.jsonl`,
  reconstructs live alloc map, peak-snapshot map, OOM event,
  `RunningStats`. Emits a `Report` via `Report::build`.
- `report.rs` — `Report` struct + render code: header, OOM section,
  overview, peak usage, peak breakdown (origin grouping with
  mechanism sub-breakdown), live allocations, allocation hotspots.
  Has a `fmt_num` thousands-separator helper.
- `resolver.rs` — `SymbolResolver` (sorted symbol table → demangled,
  shortened name lookups; `classify_alloc` for origin/mechanism
  attribution). Uses `rustc_demangle`.

### `lp-cli/src/main.rs`

Defines `Cli` enum with subcommands including `MemProfile { dir,
frames, note }` and `HeapSummary { trace_dir, top }`. Both routed in
the `main()` match.

### `lp-fw/fw-emu/Cargo.toml`

Has feature `alloc-trace = ["lp-riscv-emu-guest/alloc-trace"]`.

### `lp-riscv/lp-riscv-emu-guest/Cargo.toml`

Has feature `alloc-trace = []`.
`src/allocator.rs` — gates `TrackingAllocator` on `cfg(feature =
"alloc-trace")`.
`src/syscall.rs` — gates `ALLOC_TRACE_*` re-exports on
`cfg(feature = "alloc-trace")`.

### `lp-fw/fw-tests/tests/alloc_trace_emu.rs` (~245 LOC)

Builds fw-emu with `alloc-trace`, drives a small project end-to-end
through the emulator, calls `with_alloc_trace`/`finish_alloc_trace`,
asserts on `meta.json` and `heap-trace.jsonl` shape and that A/D
events both appear with non-decreasing `ic`.

### `justfile`

Lines 433-443: `mem-profile` and `heap-summary` recipes that wrap the
two CLI subcommands. Need updating.

### `examples/mem-profile/`

A copy of `examples/basic/` used as the default arg for
`mem-profile`. The roadmap changes `lp-cli profile`'s default arg to
`examples/basic`. Open question: do we delete `examples/mem-profile/`?
(See Q2 below.)

## Questions

Each question includes context, suggested answer, and a recommended
default. To be resolved with the user one at a time.

### Q1: Trace dir naming — keep `<timestamp>--<workload>[--<note>]/` exactly, or reserve `<mode>` slot now?

**Context.** Today's `mem-profile` writes to
`traces/<timestamp>--<dir>--<note>/`. The roadmap (`m0-foundation.md`
"Unified trace dir layout") shows the same:
`traces/<timestamp>--<workload>[--<note>]/`. But m1 onwards (per
roadmap `overview.md` and `notes.md` Q6) introduces `--mode`, and the
*notes* show `traces/<timestamp>-<workload>-<mode>[-<note>]/`
(single-dash, mode in middle). The roadmap m0 doc explicitly
specifies the double-dash form without `<mode>`.

**Suggested answer.** Match the m0 roadmap doc exactly:
`traces/<timestamp>--<workload>[--<note>]/`. m1 will insert `--<mode>`
between workload and note when mode lands; that's a m1 problem, not
m0's. Keep double-dash separator (existing convention; any tooling
that scans dir names already understands it). m0 makes zero changes
to the existing helper.

### Q2: `examples/mem-profile/` — delete, keep, or rename?

**Context.** Today the default arg to `lp-cli mem-profile` is
`examples/mem-profile`. The roadmap m0 doc changes the default arg of
`lp-cli profile` to `examples/basic`. `examples/mem-profile/` exists
as a copy of `examples/basic/` per its README ("a copy of the basic
project, allowing the basic project to be modified without affecting
the memory perf consistency"). With the unified `profile` command,
the convention going forward is "profile against the project you care
about, default `examples/basic`."

**Suggested answer.** Delete `examples/mem-profile/`. Reasons: (1) the
roadmap explicitly defaults to `examples/basic`; (2) the README
rationale ("don't perturb basic") is moot because the new tool is
about A/B against your own runs, not against a frozen baseline;
(3) keeping a stale unused fixture invites bit-rot. Risk is low
— grep for any other reference (justfile, tests) before removing.

### Q3: `Collector` trait shape — exact signatures and where do `PerfEvent` / `InstClass` / `FinishCtx` / `SyscallAction` live?

**Context.** Roadmap shows:

```rust
pub trait Collector: Send {
    fn name(&self) -> &'static str;
    fn on_syscall(&mut self, _emu: &mut Riscv32Emulator, _id: u32, _args: &[u32]) -> SyscallAction { SyscallAction::Pass }
    fn on_instruction(&mut self, _pc: u32, _kind: InstClass, _cycles: u32) {}
    fn on_perf_event(&mut self, _evt: &PerfEvent) {}
    fn finish(&mut self, ctx: &FinishCtx) -> std::io::Result<()>;
    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result;
}
```

In m0, only `on_syscall`, `finish`, `report_section` are exercised.
Issues:
- `InstClass` is an existing type
  (`lp-riscv-emu/src/emu/decoder/.../InstClass`). Available for the
  default no-op signature.
- `PerfEvent` does **not** exist yet — m1 introduces it. m0 needs
  *some* type so the trait compiles. Options:
  (a) introduce a stub `pub struct PerfEvent;` (or empty enum) in
      `profile/mod.rs` now, m1 fills it in.
  (b) omit `on_perf_event` from the trait in m0; m1 adds it.
  (c) make the method generic / take `&dyn Any` (over-engineering).
- `FinishCtx` is new; needs to carry whatever `finish()` needs to
  resolve symbols, write side files, etc. Initial fields:
  `trace_dir: &Path`, `meta_path: &Path` (or shared meta writer
  handle), and possibly the static `symbol_list` already extracted
  from the ELF.
- `SyscallAction` is new; semantics: `Pass` (let normal handling
  proceed), `Handled` (collector consumed the syscall, set return
  in regs already / no further action), `Reject` (error). For m0
  AllocCollector, `Pass` after recording is fine — the existing
  `handle_syscall` always sets `regs[A0] = 0` for `SYSCALL_ALLOC_TRACE`
  after dispatching. Likely just `Pass` and `Handled` initially.
- `&mut Riscv32Emulator` parameter on `on_syscall` is needed for
  `unwind_backtrace`. But it creates a borrow problem: the collector
  is owned by `ProfileSession` which is owned by the emulator. Need
  to either pass a non-emu reborrow context (e.g. `&Memory + regs +
  pc + cycle_count + symbol_resolver`), or temporarily move the
  session out before the call. The existing alloc-trace path uses
  `self.unwind_backtrace(self.pc, &self.regs)` then immediately
  `self.alloc_tracer.as_mut().unwrap().record_event(...)` — works
  because `unwind_backtrace` takes `&self` only and the tracer is a
  separate field. Replicating that with `ProfileSession` requires
  splitting borrows or passing an `EmuView<'_>` struct.

**Suggested answer.**
- Adopt the trait as written in the roadmap. Use stub `pub struct
  PerfEvent { /* m1: payload */ }` in `profile/mod.rs` for forward
  compat — keeps the trait surface stable across m0→m1.
- Pass an `EmuCtx<'_>` (or similar) to `on_syscall` *instead of*
  `&mut Riscv32Emulator` directly, exposing only the bits collectors
  need: `pc`, `regs`, `cycle_count`, `instruction_count`,
  `&Memory`, plus an `unwind_backtrace(&self) -> Vec<u32>` method.
  This sidesteps the borrow issue cleanly: the emulator constructs
  `EmuCtx` from its fields, then dispatches the session. Roadmap's
  literal signature `&mut Riscv32Emulator` is aspirational; the
  borrow-checker forces a slightly different shape. m1 collectors
  don't actually need `&mut Riscv32Emulator` — they need the
  observability surface above.
- `SyscallAction { Pass, Handled }` is enough for m0. (Skip `Reject`
  until a collector wants it.) `Pass` means "don't consume the
  syscall; emulator continues normal handling"; `Handled` means
  "collector took it, emulator should `Continue`."
  In m0, `AllocCollector::on_syscall` returns `Handled` for
  `SYSCALL_ALLOC_TRACE` (and the dispatcher sets regs[A0]=0 on
  `Handled`). For OOM, the dispatcher needs a way to bubble back
  `StepResult::Oom`. Options: (a) extend `SyscallAction` with
  `Halt(StepResult)`; (b) keep OOM as a side-effect on the collector
  state and have the run loop poll. (a) is cleaner — likely
  `SyscallAction { Pass, Handled, Halt(HaltReason) }` where
  `HaltReason::Oom { size }` maps to `StepResult::Oom`.
- `FinishCtx`: minimal fields for m0 — `trace_dir: &Path` and an
  iterator/handle for combined `report.txt` writing. The session
  itself owns `report.txt` (opens, asks each collector to write a
  section, closes). So `FinishCtx` may carry just `trace_dir` plus
  any shared symbol resolver. Decide concretely during design phase.

### Q4: `ProfileSession` ownership — where does it live and how is `meta.json` shared?

**Context.** The roadmap says `ProfileSession` "owns the enabled
collectors plus shared trace dir state (path, `meta.json` writer,
combined `report.txt` writer). Replaces direct `Option<AllocTracer>`
ownership on `Riscv32Emulator`."

Today, `meta.json` is written *once* by `AllocTracer::new` (pretty
JSON). With multiple collectors in m1, who writes `meta.json`? The
session needs to manage shared metadata so collectors don't fight.
For m0 with a single collector this is moot, but the design must
extend.

**Suggested answer.** `ProfileSession` writes `meta.json` itself in
its constructor. The session takes a `SessionMetadata` struct
containing the shared fields (timestamp, project, heap info, symbol
list, schema_version, plus a list of which collectors are enabled).
Collectors do **not** write `meta.json`; if a collector needs to
record collector-specific config, it goes in a sub-object keyed by
collector name (e.g. `meta.alloc = {...}`). For m0, the simplest
working version: `SessionMetadata` matches today's `TraceMetadata`
shape exactly (preserving wire format), with one extra field
`collectors: Vec<&'static str>` listing enabled collectors. Default
(when only `alloc` is enabled) writes only what alloc-trace writes
today — preserves the byte-for-byte parity goal.

`report.txt` is opened and managed by the session, not the
collectors. Session iterates enabled collectors after finish, calls
each `report_section(&mut writer)`, separates with a divider line.
Then writes the buffered string to disk *and* prints to stdout.

### Q5: `--collect` parser shape — comma-separated list, repeated flag, or both?

**Context.** Roadmap m0 says "comma-separated list; m0 only validates
`alloc`". This is forward-compatible for m1 (`--collect cpu,alloc`).
clap supports both styles.

**Suggested answer.** Comma-separated list; single `--collect <list>`
flag. clap parser deduplicates and validates against an allowed set
{`alloc`} for m0; m1 extends to {`cpu`, `alloc`, `events`}. Default
is `alloc` if `--collect` is omitted (m0 has no other choice; m1
will likely change the default to `cpu`). Whitespace around commas
is trimmed. Empty list = error. Unknown values = error with the
known-good list in the message.

### Q6: `--frames N` default — keep `10`, or align with `mem-profile`'s default?

**Context.** Today `mem-profile` defaults `--frames 10`. Roadmap m0
mirrors this exactly (`--frames N=10`). The m1 plan replaces
`--frames` with `--mode` driving a state machine.

**Suggested answer.** Keep `--frames 10` as the m0 default;
preserves behavior and migration story (any existing memorized
invocation still works). Note in the help text that the flag is
temporary and will be replaced by `--mode` in m1.

### Q7: `profile diff <a> <b>` stub — exit 0 or non-zero? What output exactly?

**Context.** Roadmap says "stub: prints 'implemented in m2';
reserved." Stubs that exit 0 risk hiding the gap from agents/CI;
stubs that exit non-zero break unrelated callers if any wire it up
prematurely.

**Suggested answer.** Exit code **2** (clap convention: usage error)
with stderr message `error: 'lp-cli profile diff' is not yet
implemented (planned for cpu-profile m2). Trace directories: <a>,
<b>`. The non-zero exit code makes accidental CI usage fail loudly.
Listing the input trace dirs in the message confirms argv parsing
worked, helping anyone porting an early script. Args still
positional-required so `--help` shows the proper signature.

### Q8: Combined `report.txt` — section divider format?

**Context.** Roadmap says "concatenates with section dividers."
Current `heap-summary` output starts with `=== Heap Trace Summary ===`
followed by sub-sections like `--- Overview ---`. To preserve
byte-for-byte parity for the alloc section while adding section
dividers, the divider must wrap *between* collectors, not inside
`heap-summary`'s existing output.

**Suggested answer.** No wrapping divider for m0 (single collector).
The m0 `report.txt` content equals exactly what `heap-summary`
prints today, byte-for-byte. The session-level "concatenate sections
with dividers" code exists but for m0 has only one section to write.
m1 (when adding the CPU section) introduces a `\n=== <Collector
Title> ===\n` divider between sections. `AllocCollector`'s
`report_section` therefore emits its content **without** a leading
collector-banner; the session decides whether to prepend one based on
how many collectors are present. For m0: no banner.

This means the alloc collector's `report_section` outputs exactly
what today's `heap-summary` `Report::render` produces.

### Q9: Cargo feature rename `alloc-trace` → `profile` — do we also rename the syscall constants?

**Context.** Roadmap says the *feature* renames to `profile` ("one
feature gates all profile-related guest-side cooperation"). The
syscall constants (`SYSCALL_ALLOC_TRACE`, `ALLOC_TRACE_ALLOC` etc.)
live in `lp-riscv-emu-shared` and are tied to the wire format. The
roadmap demands "wire format preserved byte-for-byte", which strongly
implies the syscall numbers stay the same. But the *names* could
change without breaking the wire.

**Suggested answer.** Keep syscall constant **names** unchanged
(`SYSCALL_ALLOC_TRACE` etc.). Reasons: (1) they describe the
semantics ("this is the allocation tracing syscall"), which is true
regardless of how the host-side modules are organized; (2) m1 adds
*new* syscalls (e.g. `SYSCALL_PERF_EVENT`) — different name space;
(3) renaming means churn in shared/guest code with zero behavior
benefit. The `cfg(feature = "alloc-trace")` gates in
`lp-riscv-emu-guest/src/{allocator,syscall}.rs` rename to
`cfg(feature = "profile")` to match the new feature name.

### Q10: `examples/mem-profile/` — Q2 deferred sub-question: do we update justfile recipes?

**Context.** justfile (`mem-profile` and `heap-summary` recipes)
needs updating. Two paths:
- (a) Remove the recipes entirely; users learn `cargo run -p lp-cli
  -- profile` directly.
- (b) Add a single `profile` recipe wrapping `cargo run -p lp-cli
  -- profile`.

**Suggested answer.** (b) — add `profile *args:` recipe. Removing
recipes loses the convenience; one recipe matching the new command
is cleanest. Old `mem-profile` and `heap-summary` recipes deleted.
Document the change in the recipe comment ("replaces mem-profile and
heap-summary").

### Q11: Tests — rewrite `alloc_trace_emu.rs` in place, or split into emu test + CLI smoke test?

**Context.** Today `fw-tests/tests/alloc_trace_emu.rs` exercises the
*emu-side* path (`with_alloc_trace`/`finish_alloc_trace`) end-to-end.
The roadmap calls for "existing alloc-trace tests rewired to
construct an `AllocCollector` inside a `ProfileSession`" plus a "new
CLI smoke test for `lp-cli profile --collect alloc`."

**Suggested answer.** Two tests, one each:
- Rename `fw-tests/tests/alloc_trace_emu.rs` → `profile_alloc_emu.rs`
  (or keep the old name — tests are discovered by file pattern, and
  the test function name conveys intent). Update body to construct
  `ProfileSession` with `AllocCollector`. Same assertions on
  `meta.json`/`heap-trace.jsonl` shape (proves wire-format
  preservation).
- Add a `lp-cli/tests/profile_alloc_smoke.rs` (or similar) integration
  test that exec's `cargo run -p lp-cli -- profile examples/basic
  --collect alloc --frames 2` against a tempdir-cwd, asserts
  `traces/.../heap-trace.jsonl` exists and parses, and `report.txt`
  is non-empty. This proves the new command surface works
  end-to-end.

Decision on file rename: **rename** to `profile_alloc_emu.rs` so the
filename reflects the new module organization. Slight git churn but
worth the clarity.

### Q12: Should m0 introduce a `schema_version` field in `meta.json` (per roadmap notes Q7.5)?

**Context.** Roadmap *notes* Q7.5 says `meta.json` should declare
`schema_version` and `clock_source` from day one. But the roadmap m0
doc says "wire format preserved byte-for-byte — `heap-trace.jsonl`
and `meta.json` unchanged."

These are in tension. Options:
(a) Strictly preserve `meta.json` shape (no new fields). Add
    `schema_version`/`clock_source` in m1.
(b) Add `schema_version: 1` and `clock_source: "emu_estimated"` now
    as additive fields. Existing consumers (heap-summary's
    `TraceMetaFile` deserializer) tolerate unknown fields by default
    (`#[derive(Deserialize)]` without `#[serde(deny_unknown_fields)]`).

**Suggested answer.** (a) for m0. The roadmap m0 doc is explicit:
"`heap-trace.jsonl` and `meta.json` unchanged." Adding fields counts
as changing the file. m1 introduces the schema_version/clock_source
fields when the events collector lands and `meta.json` legitimately
needs a version. m0 stays a pure refactor with zero observable
diff (apart from file *paths* moving around in source).

Note this will also keep `meta.json` parseability identical to
today's `heap-summary` if anyone reads an old trace dir post-rename.

---

## Resolved

### Q1 → `profiles/<timestamp>--<workload>[--<note>]/`

- Use double-dash separator, m0 doc format literally.
- **Top-level dir is `profiles/`, not `traces/`** — matches the
  unified `lp-cli profile` command name, drops the legacy
  `mem-profile`-era `traces/` naming. The roadmap m0/overview docs
  reference `traces/`; treat that as historical wording, the actual
  dir is `profiles/`.
- m1 inserts `--<mode>` between workload and note when `--mode`
  lands; m0 doesn't reserve the slot.
- Move existing `kebab_case` helper and naming code from
  `mem_profile/handler.rs` into `profile/handler.rs` unchanged.

### Q2 → rename `examples/mem-profile/` → `examples/perf/baseline/`

- Rename, don't delete. Reserves the project as a frozen baseline
  fixture for future A/B regression tests (e.g. m6's synthetic A/B
  suite mentioned in roadmap notes Q10).
- Use a new `examples/perf/` subdirectory namespace; future
  profile-related fixtures (regression cases, stress workloads) live
  alongside as `examples/perf/<name>/`.
- Default arg to `lp-cli profile` is still `examples/basic` (the
  live rainbow shader), per roadmap m0 doc. The renamed baseline is
  just parked for now; nothing references it.
- Grep for stray references to `examples/mem-profile` (justfile,
  tests, docs) and update or remove. The default-arg in
  `mem_profile/handler.rs` is going away anyway with the command.

### Q3 → Collector trait adopts borrow-checker-honest shape

Final trait shape for m0 (designed to absorb m1 without revision):

```rust
pub trait Collector: Send {
    fn name(&self) -> &'static str;
    fn on_syscall(&mut self, _ctx: &mut EmuCtx<'_>, _id: u32, _args: &[u32]) -> SyscallAction {
        SyscallAction::Pass
    }
    fn on_instruction(&mut self, _pc: u32, _kind: InstClass, _cycles: u32) {}
    fn on_perf_event(&mut self, _evt: &PerfEvent) {}
    fn finish(&mut self, ctx: &FinishCtx) -> std::io::Result<()>;
    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result;
}
```

Decisions:

- **`EmuCtx<'_>`** view struct (not `&mut Riscv32Emulator`).
  Constructed from emulator fields right before dispatch. Exposes
  `pc`, `regs`, `cycle_count`, `instruction_count`, `&Memory`, plus
  `unwind_backtrace(&self) -> Vec<u32>`. Sidesteps the borrow
  problem caused by the session living inside the emulator.
  Roadmap's literal `&mut Riscv32Emulator` is aspirational; this is
  the honest version.
- **`PerfEvent` is a stub `pub struct PerfEvent { /* m1 fills in */ }`**
  in `profile/mod.rs`. Default impl of `on_perf_event` is no-op;
  nothing in m0 calls it. Locks the trait shape across m0→m1.
- **`SyscallAction` and `HaltReason`:**
  ```rust
  pub enum SyscallAction { Pass, Handled, Halt(HaltReason) }
  pub enum HaltReason { Oom { size: u32 } }
  ```
  `Pass` = collector didn't claim it; emulator runs normal path.
  `Handled` = collector consumed it; emulator continues (regs[A0]
  already set by collector). `Halt` = collector wants run loop to
  stop with the given reason (m0 maps `Oom` to `StepResult::Oom`;
  m1 may add variants for mode state machine `Stop`).
- **`FinishCtx { trace_dir: &Path }`** only for m0. Collectors write
  their side files into `trace_dir`. The session owns `report.txt`
  itself (collectors only contribute via `report_section`); no
  writer handle needed in `FinishCtx`. m1 can add fields without
  breaking existing collectors.

### Q4 → ProfileSession owns trace dir + meta.json + report.txt

- `ProfileSession` writes `meta.json` itself in its constructor.
  Collectors do not write `meta.json`. Collector-specific config (if
  any) goes in a sub-object keyed by collector name (none needed for
  m0).
- `report.txt` lives entirely on the session: opens after finish,
  asks each enabled collector for its section via `report_section`,
  joins with a divider, prints to stdout, writes to disk.
- Stored on `Riscv32Emulator` as `Option<ProfileSession>`. Builder:
  `with_profile_session(SessionConfig) -> Result<Self>`.  Finalizer:
  `finish_profile_session() -> io::Result<u64>` (returns total event
  count, sum across collectors, for parity with current
  `finish_alloc_trace` return shape).
- Field layout:
  ```rust
  pub struct ProfileSession {
      trace_dir: PathBuf,
      collectors: Vec<Box<dyn Collector>>,
  }
  ```
- Constructor:
  ```rust
  pub fn new(trace_dir: PathBuf, metadata: &SessionMetadata,
             collectors: Vec<Box<dyn Collector>>) -> io::Result<Self>
  ```
- **Byte-for-byte `meta.json` preservation is dropped** (per user).
  `SessionMetadata` is free to extend with helpful fields like
  `schema_version`, `collectors: Vec<&'static str>`, etc. Specifics
  decided in Q12 below; Q4 just allows it.

### Q5 → `--collect <list>` comma-separated, defaults to `alloc`

- Single `--collect <list>` flag, comma-separated. clap config:
  `value_delimiter(',')` + `action::Append` so both `--collect a,b`
  and `--collect a --collect b` work.
- **Default = `alloc`** when flag omitted. `lp-cli profile
  examples/basic` Just Works without flags. m1 will revisit the
  default when more collectors land.
- Empty list → error. Unknown value → error message listing the
  valid set (`alloc` for m0; m1 grows it).
- Dedup silently.

### Q6 → `--frames N=10`, temporary

- Default `10`. Preserves `mem-profile` migration story.
- Help text marks it temporary: `"number of 40ms frames to run (m0
  only; replaced by --mode in m1)"`.
- u32, same plumbing as today's `mem-profile`.

### Q7 → `profile diff` stub: exit 2, stderr message

- Exit code 2 (clap usage-error convention). Loud failure prevents
  premature CI wiring.
- Stderr:
  `error: 'lp-cli profile diff' is not yet implemented (planned for cpu-profile m2)`
  followed by `trace dirs: <a>, <b>` so argv parsing is visible.
- Both positional args required so `--help` shows the proper
  signature already; m2 fills in the body.

### Q8 → per-collector banners; `report_title` trait method

- Trait gets a new method:
  ```rust
  fn report_title(&self) -> &'static str { self.name() }
  ```
  Default falls back to `name()` (the programmatic identifier);
  collectors override for human-readable titles.
- Session emits one banner per collector:
  ```
  === <report_title> ===
  <body emitted by report_section>
  ```
  Sections separated by one blank line. File ends with a trailing
  newline.
- `AllocCollector::report_title()` → `"Heap Allocation"`.
- `AllocCollector::report_section` no longer prints its own
  top-level `=== Heap Trace Summary ===` line — the session emits
  the banner. Body starts with the `--- OOM ---` / `--- Overview
  ---` sub-sections that exist today.
- m0's report.txt format is therefore *not* byte-for-byte identical
  to today's `heap-summary` output (banner wording differs); user
  explicitly OK with this.

### Q9 → rename feature only; keep syscall constant names

- Cargo feature `alloc-trace` → `profile` in:
  - `lp-fw/fw-emu/Cargo.toml`
  - `lp-riscv/lp-riscv-emu-guest/Cargo.toml`
- `cfg(feature = "alloc-trace")` → `cfg(feature = "profile")` in:
  - `lp-riscv/lp-riscv-emu-guest/src/allocator.rs`
  - `lp-riscv/lp-riscv-emu-guest/src/syscall.rs`
- Update all callers passing `--features alloc-trace` →
  `--features profile` (CLI handler, fw-tests).
- **Syscall constants keep current names**: `SYSCALL_ALLOC_TRACE`,
  `ALLOC_TRACE_{ALLOC,DEALLOC,REALLOC,OOM}`. They describe the
  syscall semantic, not the module organization. m1's new syscalls
  (e.g. `SYSCALL_PERF_EVENT`) live alongside under different names.

### Q10 → one `profile` justfile recipe replaces two

- Delete `mem-profile` and `heap-summary` recipes (justfile
  lines 433-443).
- Add single replacement:
  ```
  # Profile a project in the emulator with the unified profile collector(s).
  # Default project: examples/basic
  # Default collectors: alloc
  # Usage: just profile [path/to/project] [--collect alloc] [--frames N] [--note "description"]
  profile *args:
      cargo run -p lp-cli -- profile {{ args }}
  ```
- No standalone recipe for `profile diff` (stub only in m0).
- No re-analysis of existing trace dirs in m0 — analysis happens
  when the collector finishes; re-analyzing an old dir would
  require rerunning the workload.

### Q11 → rename emu test, add CLI smoke test

- Rename `lp-fw/fw-tests/tests/alloc_trace_emu.rs` →
  `profile_alloc_emu.rs`. Test fn renamed to
  `test_profile_alloc_produces_valid_output` (or similar).
- Body switches from `with_alloc_trace`/`finish_alloc_trace` to
  `with_profile_session`/`finish_profile_session` constructing a
  `ProfileSession` containing `AllocCollector`. All existing
  assertions on `meta.json` / `heap-trace.jsonl` content preserved
  (proves event wire format intact). Add assertion that
  `report.txt` exists and is non-empty.
- New file: `lp-cli/tests/profile_alloc_smoke.rs`. Spawns
  `cargo run -p lp-cli -- profile examples/basic --collect alloc
  --frames 2 --note ci-smoke` from a tempdir cwd. Asserts:
  - exit code 0
  - `profiles/<sess>/heap-trace.jsonl` exists + parseable JSONL
    with expected event fields
  - `profiles/<sess>/meta.json` exists + has `symbols`
  - `profiles/<sess>/report.txt` exists, non-empty, starts with
    `=== Heap Allocation ===`
- Smoke test runs unconditionally (no `#[ignore]`); build cache
  amortizes the fw-emu rebuild cost across PRs.
- `--frames 2` to keep runtime tight.

### Q12 → meta.json grouped by collector via new trait method

Trait amendment to Q3:

```rust
pub trait Collector: Send {
    // ... existing methods ...

    /// Per-collector config snapshot. Written into `meta.json` under
    /// `collectors.<name>` at session construction. Default: empty
    /// object (collector ran but contributed no config).
    fn meta_json(&self) -> serde_json::Value {
        serde_json::json!({})
    }
}
```

Session calls `meta_json()` for each enabled collector at
construction and inserts under `collectors.<name>`. Always inserted
(even `{}`) so the keys of `collectors` enumerate which ran — no
separate `collectors: ["alloc"]` array needed.

m0 `meta.json` shape:

```json
{
  "schema_version": 1,
  "timestamp": "...",
  "project": "test-project",
  "workload": "examples/basic",
  "note": null,
  "clock_source": "emu_estimated",
  "frames_requested": 10,
  "symbols": [ { "addr", "size", "name" }, ... ],
  "collectors": {
    "alloc": { "heap_start": 2147483648, "heap_size": 327680 }
  }
}
```

Placement rules:
- **Top-level (session-owned):** `schema_version`, `timestamp`,
  `project`, `workload`, `note`, `clock_source`, `frames_requested`
  (m0 run-config; m1 replaces with `mode`), `symbols` (shared
  across any collector that records PCs).
- **`collectors.<name>`:** collector-specific config (alloc keeps
  `heap_start`/`heap_size`).

m1 collectors slot in: `collectors.cpu = { "cycle_model": "esp32c6" }`,
`collectors.events = {}`.

`AllocCollector::report_section` reads from the m0 fields it needs
(`frames_requested` from top-level; `heap_start`/`heap_size` from
its own sub-object; `symbols` from top-level). Symbol resolver gets
re-pointed at the new path.

## Notes

(Populated as questions are resolved.)
