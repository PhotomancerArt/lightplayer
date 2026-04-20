### What was built

- `JitSymbolEntry` ABI (`#[repr(C)]`, four `u32`s) added to `lp-riscv-emu-shared`; `SYSCALL_JIT_MAP_LOAD` (11) and `SYSCALL_JIT_MAP_UNLOAD` (12) docstrings updated.
- `lp_perf::emit_jit_map_load(base, len, &[JitSymbolEntry])` added with sink dispatch (`syscall` ecall on RV32, `log::debug!`, noop).
- `lpvm-native` calls `emit_jit_map_load` once per successful JIT link from `compile_module_jit`; sizes derived from sorted-offset deltas (helper in new `lp-shader/lpvm-native/src/jit_symbol_sizes.rs`).
- Host: `SYSCALL_JIT_MAP_LOAD` handler in `run_loops.rs` reads guest memory, parses entries, calls `ProfileSession::on_jit_map_load`. `SYSCALL_JIT_MAP_UNLOAD` is a `log::error!` stub.
- New `JitSymbols` overlay (`lp-riscv/lp-riscv-emu/src/profile/jit_symbols.rs`) owned by `ProfileSession`; rewrites `meta.json` with `dynamic_symbols` at `finish_with_symbolizer` time.
- `lp-cli` profile `Symbolizer` now takes static + dynamic symbols; new public `symbolizer_from_meta_json_str`. `handle_profile` snapshots `session.jit_symbols()` before finish so `report.txt` resolves JIT names; speedscope and `cpu-profile.json` re-load `meta.json` after finish.
- Alloc-trace `SymbolResolver` (`profile/alloc.rs`) merges `dynamic_symbols` from `meta.json`; static wins on overlap.
- New end-to-end test `fw-tests/tests/profile_jit_symbols_emu.rs` proves the chain (JIT emit → ecall → host → meta.json → symbolizer) resolves a JIT'd PC to the expected shader function name.
- `docs/roadmaps/2026-04-19-cpu-profile/m6-validation-docs.md` gets a "Known limitation from m5" note covering both UNLOAD-not-implemented and the unrelated gap that `fw-esp32` does not enable `lp-perf/syscall`.
- `docs/future-work/2026-04-20-jit-symbols-in-panic-backtrace.md` records the deferred panic-backtrace symbolization scope.

### Decisions for future reference

#### `dynamic_symbols` schema carries cycle metadata even though m5 ignores it

- **Decision:** Each `dynamic_symbols` entry includes `loaded_at_cycle: u64` and `unloaded_at_cycle: u64 | null`, even though m5 lookups are cycle-insensitive (latest-inserted wins on overlap).
- **Why:** Future timeline-aware lookup or real `SYSCALL_JIT_MAP_UNLOAD` wiring should land without a `meta.json` schema migration.
- **Rejected alternatives:** Pure `(addr, size, name)` rows (cleaner today, but every reader would need to re-handle absence later).
- **Revisit when:** UNLOAD or per-cycle symbol windowing is implemented.

#### `lp-perf` is the seam for JIT symbol emission, not a new `lpvm-native` feature

- **Decision:** Reuse the existing `lp-perf` sink machinery (`syscall` / `log` / `noop`) for `emit_jit_map_load`. No new feature plumbed through `fw-emu → lp-engine → lpvm-native`.
- **Why:** `lp-perf` already chooses the right sink per build (RV32 firmware vs. host vs. test), and `lp-perf/syscall` already depends on `lp-riscv-emu-shared`. Adding a parallel `profile`-style feature would have meant cascading flag forwarding through 3-4 crates.
- **Rejected alternatives:** New `lpvm-native/profile` feature (more plumbing, no benefit); direct `core::arch::asm!` in `lpvm-native` (duplicates the cfg gating `lp-perf` already does).
- **Revisit when:** A second sink-style channel needs to emit from inside the JIT for reasons unrelated to perf events.

#### `meta.json` is rewritten at session finish, not streamed during the run

- **Decision:** `JitSymbols` accumulates in memory; `finish_with_symbolizer` reads `meta.json` from disk once, splices in `dynamic_symbols`, and writes back.
- **Why:** Keeps the on-disk file consistent (no half-written intermediate states) and there is no consumer that needs `dynamic_symbols` mid-run. Cost is dominated by the report.txt build that already runs at finish.
- **Rejected alternatives:** Stream a `dynamic-symbols.jsonl` like `heap-trace.jsonl` (extra file, no consumer); rewrite `meta.json` on every `add_module` (gratuitous fsync churn).
- **Revisit when:** A live consumer (e.g. an interactive profiler UI watching the trace dir) needs to see new JIT symbols before finish.

#### Panic-backtrace JIT names deferred (separate symbol-resolution path)

- **Decision:** Out of scope for m5. The emulator's panic backtrace path uses `lp-riscv-elf::format_backtrace`, which has its own resolver and does not consult the profile-side dynamic symbols.
- **Why:** Wiring it would require API changes in a separate crate and a different plumbing path; no value users are blocked on it for m5's flame-chart-naming goal.
- **Rejected alternatives:** Plumb `JitSymbols` through to `format_backtrace` now (scope creep, design pressure on `lp-riscv-elf`'s API).
- **Revisit when:** Devs are debugging a JIT'd-code crash and the `<jit:0x...>` backtrace lines are the bottleneck. See `docs/future-work/2026-04-20-jit-symbols-in-panic-backtrace.md`.

#### `fw-esp32` not yet wired to `lp-perf/syscall` — recorded, not fixed

- **Decision:** m5 does not change `fw-esp32`'s `lp-perf` features. As shipped, `fw-esp32` uses `lp-perf` with `default-features = false` (no `syscall` sink), so JIT symbols won't reach the host overlay for ESP32-captured profiles.
- **Why:** m5's deliverable was the chain itself, validated end-to-end on `fw-emu`. Enabling the syscall sink on ESP32 may have firmware-binary-size implications that deserve their own investigation.
- **Rejected alternatives:** Force `fw-esp32` to enable `lp-perf/syscall` as part of m5 (untested binary-size impact; not the m5 deliverable).
- **Revisit when:** ESP32 device profiling needs symbolized flame charts. Note recorded in `m6-validation-docs.md`.
