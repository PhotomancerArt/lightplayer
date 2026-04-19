# Milestone 5: JIT Symbols

## Goal

Symbolize JIT'd shader code so flame charts show real shader function
names (`palette_warm`, `psrd_noise`) instead of `<jit:0x80a4f10>`. The
JIT runtime tells the host about each linked module's name table via
a new `SYSCALL_JIT_MAP_LOAD`; the host maintains a dynamic-symbol
overlay consulted alongside ELF symbols at attribution time.

Bonus payoff that lands automatically with this milestone: panic
backtraces and alloc-trace reports also start showing real shader
function names instead of unsymbolized `<unknown 0x...>`.

## Suggested Plan Name

`profile-m5-jit-symbols`

## Scope

### In scope

- **`SYSCALL_JIT_MAP_LOAD` implementation.** The constant was
  reserved in m1; m5 wires up the syscall handler.

  ABI: `(base_addr: u32, len: u32, count: u32, entries_ptr: u32)`
  where each entry at `entries_ptr` is a `repr(C)` struct:

  ```rust
  #[repr(C)]
  struct JitSymbolEntry {
      offset: u32,     // offset within the JIT module
      size: u32,       // function size in bytes
      name_ptr: u32,   // guest pointer to UTF-8 string
      name_len: u32,
  }
  ```

  Host reads the entries array, constructs symbol records tagged
  with `loaded_at_cycle = self.cycle_count`, appends to the dynamic
  symbol overlay.

- **JIT runtime emission** in `lp-shader/lpvm-native/src/rt_jit/module.rs`.
  After `link_jit` succeeds, the runtime gathers the function table
  (already known at link time — this is what the linker resolved
  against), builds the `JitSymbolEntry` array, calls
  `SYSCALL_JIT_MAP_LOAD`. Emission is `#[cfg(feature = "profile")]`
  gated so non-profiling builds pay zero cost.

- **`JitSymbols` overlay** in new file
  `lp-riscv-emu/src/profile/jit_symbols.rs`:

  ```rust
  pub struct JitSymbols {
      entries: Vec<JitSymbolRecord>,
  }
  
  struct JitSymbolRecord {
      base_addr: u32,
      offset: u32,
      size: u32,
      name: String,
      loaded_at_cycle: u64,
      unloaded_at_cycle: Option<u64>,   // always None in m5; reserved for future
  }
  
  impl JitSymbols {
      pub fn add_module(&mut self, base: u32, len: u32, entries: &[(u32, u32, String)], cycle: u64);
      pub fn lookup(&self, pc: u32) -> Option<&str>;
  }
  ```

  In m5, `lookup` does a flat scan (simple linear search; tens of
  symbols expected). Interval-aware lookup (cycle-keyed) deferred —
  current shader pipeline doesn't unload modules.

- **Symbolizer integration.** `lp-cli/src/commands/profile/symbolize.rs`
  (introduced in m2) extended:

  ```rust
  pub fn resolve(pc: u32, static_syms: &[Symbol], jit_syms: &JitSymbolOverlay) -> String {
      static_syms.lookup(pc)
          .or_else(|| jit_syms.lookup(pc))
          .map(|name| name.to_string())
          .unwrap_or_else(|| format!("<unknown 0x{:08x}>", pc))
  }
  ```

  Static symbols win on PC overlap (shouldn't happen in practice but
  static is canonical).

- **JIT symbols persisted to `meta.json`.** After run completes, the
  emulator's `JitSymbols` overlay is serialized into `meta.json` as
  a `dynamic_symbols` array alongside the existing static `symbols`
  array. `lp-cli`'s symbolizer reads both from disk for
  post-processing tools (re-running diff with new symbol info).

  ```json
  {
    "schema_version": 1,
    ...
    "symbols": [ { "addr": 0x40380000, "size": 256, "name": "render::frame" }, ... ],
    "dynamic_symbols": [ { "addr": 0x80a40010, "size": 124, "name": "palette_warm" }, ... ]
  }
  ```

- **`SYSCALL_JIT_MAP_UNLOAD` reserved but not implemented.**
  Constant exists from m1; the syscall handler in m5 logs an error
  if invoked ("UNLOAD not yet implemented; symbols may be stale").
  No JIT-runtime call site emits it. Documented in
  `docs/design/native/fw-profile/` (m6) as a known limitation.

- **Bonus: panic backtraces and alloc-trace use the new symbols.**
  Both already format PCs through symbol lookup; once
  `JitSymbols` is consulted, they show shader function names too.
  No code change needed in panic handler or alloc-trace; just the
  symbolizer.

- **Tests.**
  - Unit test for `JitSymbols::add_module` and `lookup`.
  - Unit test for syscall handler: synthetic guest memory layout +
    syscall args → expected `JitSymbols` state.
  - Unit test for symbolizer: PC in JIT region resolves to JIT
    symbol; PC in ELF region resolves to ELF symbol; unknown PC
    formats as `<unknown 0x...>`.
  - Integration test: `lp-cli profile examples/basic --collect cpu`
    produces a `report.txt` and `cpu-profile.speedscope.json` where
    JIT'd shader functions appear with real names (e.g.
    `palette_warm`), not `<jit:0x...>`.

### Out of scope

- `SYSCALL_JIT_MAP_UNLOAD` implementation (reserved; deferred until
  hot-reload lands or a real collision is observed).
- Per-basic-block symbolization (function-level only — matches what
  every other profiler does).
- Symbolizing the LPIR-level structure (showing IR ops in the flame
  chart). Possible future-work; out of scope.
- Source-line attribution from DWARF in JIT'd code. Function-level
  is sufficient.

## Key Decisions

- **Symbolize at function granularity.** Matches what every other
  profiler does and is what's actionable. Per-BB or per-line is
  future-work.

- **Static symbols win on PC overlap.** Defensive — static ELF
  symbols are canonical and should never collide with JIT-region
  PCs anyway. The `or_else` order makes it explicit.

- **`UNLOAD` deferred.** Current shader pipeline doesn't unload
  modules. Implementing the timeline-aware lookup adds complexity
  for zero current benefit. Reserved syscall number means it can
  land later without ABI churn.

- **JIT symbols persisted to `meta.json`.** Important for
  post-processing — re-running `profile diff` with archive data
  needs the symbol overlay available offline. Cheap (kilobytes).

- **JIT runtime emission gated on `feature = "profile"`.** Same
  feature flag that gates alloc-trace and perf-event emission.
  Non-profiling builds pay zero cost.

- **Bonus payoff is real, not just theoretical.** The same
  symbolizer is used by panic backtraces and alloc-trace today.
  Adding JIT symbols benefits all three reporting paths
  simultaneously.

- **Linear search for lookup.** Tens of symbols expected (one
  shader, maybe a few helpers). Profiling output isn't a hot path.
  Optimize when there's evidence it matters.

## Deliverables

### `lp-riscv-emu-shared` crate
- `SYSCALL_JIT_MAP_LOAD` and `SYSCALL_JIT_MAP_UNLOAD` constants
  (reserved in m1) — comments updated to note current
  implementation status.

### `lp-riscv-emu` crate
- New: `lp-riscv-emu/src/profile/jit_symbols.rs` — `JitSymbols`
  overlay.
- Updated: `lp-riscv-emu/src/profile/mod.rs` —
  `ProfileSession` owns a `JitSymbols`; serializes it into
  `meta.json` at finish.
- Updated: `lp-riscv-emu/src/emu/emulator/run_loops.rs` —
  `SYSCALL_JIT_MAP_LOAD` handler reads entries from guest memory,
  appends to overlay.
- Updated: `lp-riscv-emu/src/emu/emulator/run_loops.rs` —
  `SYSCALL_JIT_MAP_UNLOAD` handler logs error.
- Updated: backtrace path (the existing `unwind_backtrace` user)
  consults `JitSymbols` for symbol names if it doesn't already.
- Updated: alloc-trace metadata serialization picks up
  `dynamic_symbols`.

### `lp-shader` crate
- Updated: `lp-shader/lpvm-native/src/rt_jit/module.rs` — emits
  `SYSCALL_JIT_MAP_LOAD` after successful `link_jit`. Gated on
  `feature = "profile"`.

### `lp-cli` crate
- Updated: `lp-cli/src/commands/profile/symbolize.rs` — consults
  `dynamic_symbols` from `meta.json` in addition to static symbols.

### Tests
- Unit tests in `lp-riscv-emu/src/profile/jit_symbols.rs`.
- Unit tests for syscall handlers in `run_loops.rs`.
- Integration test: end-to-end `profile --collect cpu` shows JIT
  symbol names in outputs.

## Dependencies

- m0, m1 (syscall constants reserved), m2 (CPU collector and
  symbolizer to extend).
- m3 not strictly required but landing m5 after m3 means diff
  reports also benefit from JIT symbols.
- m4 not required; can land in any order relative to m4.

## Validation

```bash
# Workspace builds
cargo build --workspace

# fw-esp32 / fw-emu still build (profile feature paths exercised)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server
cargo build -p fw-emu --target riscv32imac-unknown-none-elf \
  --profile release-emu --features profile

# Unit tests
cargo test -p lp-riscv-emu

# End-to-end: JIT symbols visible in profile outputs
cargo run -p lp-cli --release -- profile examples/basic --collect cpu
# Expected: report.txt's top-N shows shader function names like
# `palette_warm`, `psrd_noise` — not `<jit:0x...>`.
# meta.json has populated `dynamic_symbols` array.

# Bonus: alloc-trace also shows JIT symbols
cargo run -p lp-cli --release -- profile examples/basic --collect alloc
# Expected: heap-trace.jsonl frames with PCs in JIT region resolve
# to shader function names in the heap-summary section.

# Composability
cargo run -p lp-cli --release -- profile examples/basic \
  --collect cpu,alloc
# Both reports show JIT symbols in their respective sections.

# Regression: m3 diff still works
cargo run -p lp-cli --release -- profile diff <a> <b>
# JIT-region functions now match by name across runs (previously
# matched by PC, which was unreliable due to JIT layout drift).
```

## Estimated Scope

- New code: ~400-600 LOC.
  - `JitSymbols` overlay: ~150.
  - Syscall handler in `run_loops.rs`: ~100.
  - JIT runtime emission in `lpvm-native::rt_jit::module`: ~80.
  - Symbolizer extension: ~50.
  - `meta.json` schema update: ~50.
- Tests: ~250-400 LOC.
- Files touched: ~8-12.

## Agent Execution Notes

Implementation order:

1. Read `lp-shader/lpvm-native/src/rt_jit/module.rs` and
   `link_jit` flow to understand what symbol info is available
   at link time.
2. Read existing static-symbol handling in
   `lp-cli/src/commands/profile/symbolize.rs` to understand the
   extension point.
3. Read the `SYSCALL_*` handler pattern in
   `lp-riscv-emu/src/emu/emulator/run_loops.rs`.
4. Implement `JitSymbols` overlay pure-functionally with unit
   tests.
5. Implement `SYSCALL_JIT_MAP_LOAD` handler — reads entries from
   guest memory, appends to overlay.
6. Stub `SYSCALL_JIT_MAP_UNLOAD` handler that logs an error.
7. Wire `JitSymbols` serialization into `meta.json` at session
   finish.
8. Extend `lp-cli` symbolizer to consult `dynamic_symbols`.
9. Implement JIT runtime emission in `lpvm-native::rt_jit::module`.
10. End-to-end test: profile `examples/basic`, verify shader
    function names appear in outputs.
11. Verify bonus payoff: panic + alloc-trace also show JIT
    names.
