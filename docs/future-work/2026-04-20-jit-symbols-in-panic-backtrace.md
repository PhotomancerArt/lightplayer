# JIT symbols in emulator panic backtraces

## Status: deferred from cpu-profile m5

m5 wires JIT-emitted symbol names into the profile reporting paths
(`lp-cli`'s `Symbolizer` and the alloc-trace `SymbolResolver`), but
**not** into emulator panic backtraces. Panic frames whose PC falls
in the JIT region (`>= 0x8000_0000`) still print as `???` /
`(invalid address)` per `lp_riscv_elf::format_backtrace`'s current
fallback.

The roadmap originally claimed this was a "free bonus" of m5; that
turned out not to be true (see "Why deferred" below).

## Scope of the deferred work

`lp-core/lp-client/src/transport_emu_serial.rs` calls
`lp_riscv_elf::format_backtrace(&addrs, &HashMap<String, u32>,
code_end)` after an emulator-side error. To pick up JIT names this
needs:

- A signature widening on `lp_riscv_elf::format_backtrace` (or a new
  variant) that accepts a dynamic-symbol overlay alongside the
  static name → addr `HashMap`.
- A way to plumb the live `JitSymbols` overlay from the running
  `Riscv32Emulator` to the formatter at panic time. A panic does
  **not** require an active `ProfileSession`, so this can't piggy-
  back on the session's overlay; the emulator itself would need to
  own (or hand out a clone of) the symbol table.
- A revisit of `code_end` (currently a single boundary marking the
  end of the static image). JIT regions are scattered in heap and
  outside that boundary; the formatter's "invalid address" check
  has to learn to consult the dynamic overlay before declaring an
  address invalid.

Not huge — probably ~150 LOC plus a tiny plumbing PR through
`lp-client` — but it's a separate concern (panic path, not profile
path) and a separate crate (`lp-riscv-elf`).

## Why deferred

- (1) profile reports and (2) the alloc-trace `SymbolResolver` both
  read `dynamic_symbols` from `meta.json` after a session finishes.
  One on-disk array, two consumers, a few lines each.
- (3) panic backtraces happen *during* a normal serial-transport
  session, with no profile session active, so they need the
  overlay handed in via a different path. That's a different
  shape of change and deserves its own focused plan.

## Picking it up later

Likely shape of a follow-up:

1. Add `lp_riscv_elf::DynamicSymbol { addr, size, name }` and a
   `format_backtrace_with_dynamic` variant (or widen the existing
   signature).
2. Have `Riscv32Emulator` own an `Option<Arc<JitSymbols>>` populated
   by the same `SYSCALL_JIT_MAP_LOAD` path that already feeds
   `ProfileSession`.
3. `transport_emu_serial.rs` reads it from the emulator at error
   time and hands it to the formatter.

Document removal: when this lands, remove the "known limitation"
note added in cpu-profile m6 docs.
