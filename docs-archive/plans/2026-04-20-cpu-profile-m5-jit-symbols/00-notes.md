# m5 JIT Symbols — Notes

Roadmap: `docs/roadmaps/2026-04-19-cpu-profile/m5-jit-symbols.md`.

## Scope of work

Symbolize JIT'd shader code so profile reports show real shader function
names (`palette_warm`, `psrd_noise`) instead of `<jit:0x80a4f10>`.

Three slices:

1. **Guest emission.** After `compile_module_jit` succeeds in
   `lp-shader/lpvm-native/src/rt_jit/`, the JIT runtime tells the host
   about the linked module's symbol table via a new
   `SYSCALL_JIT_MAP_LOAD` (constant 11, already reserved in
   `lp-riscv/lp-riscv-emu-shared/src/syscall.rs`).
2. **Host overlay.** A new `JitSymbols` overlay in
   `lp-riscv/lp-riscv-emu/src/profile/jit_symbols.rs` owned by
   `ProfileSession`. The `SYSCALL_JIT_MAP_LOAD` handler in
   `lp-riscv-emu/src/emu/emulator/run_loops.rs` reads entries from
   guest memory and appends to it. The overlay is persisted to disk
   alongside the existing `symbols` array so post-processing tools
   (`lp-cli profile diff` once m3 lands) can use it offline.
3. **Symbolizer fallback.** `lp-cli/src/commands/profile/symbolize.rs`
   consults the dynamic-symbol overlay when a PC misses the static
   ELF symbols. Static still wins on overlap.

Bonus payoff (in scope to the extent it's cheap): the alloc-trace
report in `lp-riscv-emu/src/profile/alloc.rs` and the panic backtrace
in `lp-core/lp-client/src/transport_emu_serial.rs` should also show
shader function names. The bonus is **not** automatic — both paths
have their own symbol resolvers. Scope decided in Q10.

`SYSCALL_JIT_MAP_UNLOAD` (constant 12, also reserved) stays a
logged-error stub in m5; current shader pipeline doesn't unload
modules.

## Current state of the codebase

### Syscall constants (already reserved in m1)

`lp-riscv/lp-riscv-emu-shared/src/syscall.rs`:

```rust
pub const SYSCALL_JIT_MAP_LOAD: i32 = 11;
pub const SYSCALL_JIT_MAP_UNLOAD: i32 = 12;
```

Both are reserved-but-unimplemented as of m1.

### JIT compile/link pipeline

`lp-shader/lpvm-native/src/rt_jit/compiler.rs::compile_module_jit`
returns `(JitBuffer, BTreeMap<String, usize>, ModuleDebugInfo)`,
where the `BTreeMap<String, usize>` is `entries: function name →
byte offset within the JIT buffer`. There is no per-function size
tracked separately; sizes are derivable from sorted-offset deltas
(last function size = `buffer.len() - last_offset`).

`JitBuffer` (`lp-shader/lpvm-native/src/rt_jit/buffer.rs`) wraps
`Vec<u8>`. The host PC for an entry is
`buffer.code.as_ptr() as u32 + offset` — on RV32 firmware, host
pointer == guest address. The buffer is owned by the
`Arc<NativeJitModuleInner>` in `NativeJitModule`, so symbol-name
strings live for the module's lifetime.

`lp_perf::emit_begin!(EVENT_SHADER_LINK)` / `emit_end!` already
brackets the link step inside `compile_module_jit` (line 62 / 71)
so we have a precedent for using `lp-perf` from inside the JIT
runtime.

### Host syscall dispatch

`lp-riscv/lp-riscv-emu/src/emu/emulator/run_loops.rs::handle_syscall`
is a long `if/else` chain over syscall numbers. New syscalls are
added by adding an `else if syscall_info.number == SYSCALL_FOO`
branch. `SYSCALL_PERF_EVENT` (handled via
`Riscv32Emulator::handle_perf_event_syscall`, lines 53-120) is the
closest precedent — it parses guest-memory args, dispatches to
`ProfileSession`, and returns `StepResult::Continue`. It runs only
under `#[cfg(feature = "std")]`.

`ProfileSession` (`lp-riscv-emu/src/profile/mod.rs`) owns
`collectors`, `gate`, and `halt_reason` today. It does **not**
own a symbol overlay. JIT symbols would be a new field on
`ProfileSession`.

### `meta.json` lifecycle

`ProfileSession::new` writes `meta.json` once, at session start,
including the static `symbols: Vec<TraceSymbol>`. There is no
re-write path today. To persist JIT symbols (loaded throughout the
run) we either rewrite `meta.json` at finish or use a sibling file.
See Q9.

### lp-perf sink architecture

`lp-base/lp-perf/src/sinks/{mod.rs,syscall.rs,log_sink.rs,noop.rs}`
selects one of three sinks at compile time via mutually exclusive
`syscall` / `log` / (none → noop) features. The `syscall` feature
already pulls `lp-riscv-emu-shared` for the `SYSCALL_PERF_EVENT`
constant. fw-emu sets `lp-perf/syscall`; fw-esp32 also sets
`lp-perf` (host build sees `noop`).

Today, `lp-perf::__emit(name, kind)` is the only emit API. We will
add a parallel `emit_jit_map_load(base, len, entries)` API
following the same sink dispatch pattern.

### Existing static symbolizer

`lp-cli/src/commands/profile/symbolize.rs::Symbolizer` builds a
sorted `Vec<(lo, hi, name)>` from `&[TraceSymbol]` and does
`partition_point` interval lookup. Misses in RAM
(`pc >= 0x8000_0000`) format as `<jit:{pc:#010x}>` — exactly the
strings we want to replace. Extending this is straightforward:
chain a second sorted-interval lookup (over JIT entries) before
the final `<jit:...>` fallback.

The alloc-trace report has its own `SymbolResolver` in
`lp-riscv-emu/src/profile/alloc.rs` (lines 731-789) that loads the
static `symbols` array directly from `meta.json` and does its own
binary search. It does **not** consult the lp-cli `Symbolizer`.

### Panic backtrace (separate path)

`lp-core/lp-client/src/transport_emu_serial.rs` calls
`lp_riscv_elf::format_backtrace(&addrs, &HashMap<String, u32>,
code_end)` (lines 152-153). The signature takes a static
`HashMap<String, u32>` keyed by name → addr, with no notion of a
dynamic overlay or PC-to-name reverse lookup at all (it builds a
reverse map internally). Adding JIT support means widening this
API and the firmware's `code_end` notion of what's "in the
image". Non-trivial for a "free bonus".

## Questions

### Confirmation-style (answered)

| #   | Question                                                                       | Answer |
| --- | ------------------------------------------------------------------------------ | ------ |
| Q1  | Plan dir `docs/plans/2026-04-20-cpu-profile-m5-jit-symbols/`?                  | Yes    |
| Q2  | Use `profiles/` (not `traces/`)?                                               | Yes    |
| Q3  | Static symbols win on PC overlap (`static.or_else(jit)`)?                      | Yes    |
| Q4  | Linear scan for `JitSymbols::lookup`?                                          | Yes    |
| Q5  | `SYSCALL_JIT_MAP_UNLOAD` stays a logged-error stub in m5?                      | Yes    |
| Q6  | Compute per-function `size` from sorted offset deltas in `compile_module_jit`? | Yes    |
| Q7  | Function-granular only (no per-BB or DWARF source lines)?                      | Yes    |

### Discussion-style

#### Q8: How to gate `SYSCALL_JIT_MAP_LOAD` emission. — answered

Extend `lp-perf` with a parallel emit path, mirroring how
`SYSCALL_PERF_EVENT` is gated today.

- Add `pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry])`
  to `lp-perf::lib.rs`, dispatching through `sinks::emit_jit_map_load`.
- `sinks/syscall.rs` (RV32 only) emits `ecall` with
  `SYSCALL_JIT_MAP_LOAD` and `(base, len, count, entries_ptr)` in
  `a0..a3`.
- `sinks/log_sink.rs` writes `log::debug!`.
- `sinks/noop.rs` is empty.
- `JitSymbolEntry` (`#[repr(C)] { offset, size, name_ptr, name_len }`)
  is defined in `lp-riscv-emu-shared` (next to
  `SYSCALL_JIT_MAP_LOAD`); both `lp-perf::sinks::syscall` and the
  host `run_loops.rs` handler import it from there.
- `compile_module_jit` calls `lp_perf::emit_jit_map_load(...)` after
  constructing `JitBuffer`. No new Cargo feature on `lpvm-native`,
  no plumbing through `fw-emu → fw-core → lp-engine → lpvm-native`.
- fw-emu (with `lp-perf/syscall`) emits the real ecall; fw-esp32
  does the same; host builds and tests get `noop`.

#### Q9: Where to persist the JIT symbol overlay. — answered

Rewrite `meta.json` at `ProfileSession::finish_with_symbolizer`,
adding a top-level `dynamic_symbols: [...]` field. Single file,
single schema, matches the roadmap's example. Re-serialize cost is
negligible (kilobytes).

Per-record schema carries the cycle metadata that the in-memory
`JitSymbolRecord` already tracks — cheapest forward-compat that
pays off, no commitment to event/module grouping:

```json
"dynamic_symbols": [
  {
    "addr": 2158886928,
    "size": 124,
    "name": "palette_warm",
    "loaded_at_cycle": 12345,
    "unloaded_at_cycle": null
  }
]
```

For m5: `unloaded_at_cycle` is always `null` (UNLOAD deferred). The
CLI `Symbolizer` in m5 ignores both cycle fields and does a flat
latest-wins lookup. The fields are carried so a future timeline-
aware lookup, hot-reload, or UNLOAD wiring can use them without a
schema migration.

Implementation:

- `ProfileSession` gets a `jit_symbols: JitSymbols` field.
- `SYSCALL_JIT_MAP_LOAD` handler appends new records into it.
- `finish_with_symbolizer` reads `meta.json` back, splices in
  `dynamic_symbols`, writes it back.

#### Q10: Scope of the "bonus payoff". — answered

In scope for m5:

- **Profile reports** — `lp-cli/src/commands/profile/symbolize.rs`
  extended to consult `dynamic_symbols` after the static
  `symbols`. Single shared overlay loaded from `meta.json`.
- **Alloc-trace heap-summary** —
  `lp-riscv-emu/src/profile/alloc.rs::SymbolResolver` extended to
  also deserialize `dynamic_symbols` from `meta.json` and merge
  into the same sorted interval list.

Out of scope for m5:

- **Emulator panic backtraces** —
  `lp-core/lp-client/src/transport_emu_serial.rs` →
  `lp_riscv_elf::format_backtrace`. Requires a signature change in
  a separate crate plus live-overlay plumbing (panics happen with
  no profile session active). Deferred — see
  `docs/future-work/2026-04-20-jit-symbols-in-panic-backtrace.md`.
  Mention as a known limitation when m6 docs land.

# Notes

- Dependency added by Q8: `lp-perf` gains a parallel emit path. The
  ABI struct `JitSymbolEntry` is defined in `lp-riscv-emu-shared`
  next to `SYSCALL_JIT_MAP_LOAD`; both `lp-perf::sinks::syscall`
  (guest) and the host `run_loops.rs` handler import it from there.
- Dependency added by Q9: `ProfileSession` re-reads + rewrites
  `meta.json` at finish to splice in `dynamic_symbols`. New helper
  on the session, not on each collector.
- Future work doc: `docs/future-work/2026-04-20-jit-symbols-in-panic-backtrace.md`.
