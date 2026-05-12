# m5 JIT Symbols — Design

Roadmap: `docs/roadmaps/2026-04-19-cpu-profile/m5-jit-symbols.md`.
Notes (questions, current state): `00-notes.md`.

## Scope of work

Symbolize JIT-emitted shader code so profile reports show real shader
function names (`palette_warm`, `psrd_noise`) instead of
`<jit:0x80a4f10>`. The JIT runtime tells the host about each linked
module's symbol table via `SYSCALL_JIT_MAP_LOAD`; the host maintains a
dynamic-symbol overlay persisted into `meta.json` and consulted by the
profile symbolizer alongside static ELF symbols.

**In scope:**

- `SYSCALL_JIT_MAP_LOAD` ABI definition, host handler, guest emission
  via `lp-perf`.
- `JitSymbols` overlay owned by `ProfileSession`.
- `dynamic_symbols` array in `meta.json` (rewritten at session finish).
- `lp-cli` profile `Symbolizer` consults static + dynamic.
- `lp-riscv-emu/src/profile/alloc.rs::SymbolResolver` consults
  static + dynamic.
- `SYSCALL_JIT_MAP_UNLOAD` handler is a logged-error stub (constant
  reserved, no call site emits it).
- Unit tests + an end-to-end check that JIT names appear in
  `report.txt`.

**Out of scope:**

- `SYSCALL_JIT_MAP_UNLOAD` real implementation.
- Per-basic-block or DWARF-line symbolization (function-granular only).
- LPIR-level symbolization in flame charts.
- Emulator panic-backtrace JIT names — see
  `docs/future-work/2026-04-20-jit-symbols-in-panic-backtrace.md`.

## File structure

```
lp-base/lp-perf/
├── Cargo.toml                                   # UPDATE: re-export JitSymbolEntry under syscall feature
└── src/
    ├── lib.rs                                   # UPDATE: add emit_jit_map_load() entry point
    └── sinks/
        ├── mod.rs                               # UPDATE: re-export emit_jit_map_load from selected sink
        ├── syscall.rs                           # UPDATE: ecall SYSCALL_JIT_MAP_LOAD on RV32, no-op elsewhere
        ├── log_sink.rs                          # UPDATE: log::debug!("JIT module loaded ...")
        └── noop.rs                              # UPDATE: empty emit_jit_map_load

lp-riscv/lp-riscv-emu-shared/
└── src/
    ├── syscall.rs                               # UPDATE: drop "not yet implemented" comment on SYSCALL_JIT_MAP_LOAD
    ├── jit_symbol_entry.rs                      # NEW: #[repr(C)] struct JitSymbolEntry
    └── lib.rs                                   # UPDATE: pub mod jit_symbol_entry; re-export

lp-riscv/lp-riscv-emu/src/profile/
├── mod.rs                                       # UPDATE: ProfileSession owns JitSymbols; meta.json rewrite at finish
├── jit_symbols.rs                               # NEW: JitSymbols overlay, JitSymbolRecord, lookup, serialization
└── alloc.rs                                     # UPDATE: SymbolResolver also loads dynamic_symbols

lp-riscv/lp-riscv-emu/src/emu/emulator/
└── run_loops.rs                                 # UPDATE: SYSCALL_JIT_MAP_LOAD handler, UNLOAD logged-error stub

lp-shader/lpvm-native/src/rt_jit/
└── compiler.rs                                  # UPDATE: after build, call lp_perf::emit_jit_map_load

lp-cli/src/commands/profile/
└── symbolize.rs                                 # UPDATE: Symbolizer takes static + dynamic, chains lookups
```

Total: 2 new files, ~10 updated files.

## Conceptual architecture

```
                    GUEST (RV32 firmware: fw-emu / fw-esp32)
                    ─────────────────────────────────────────

              compile_module_jit() in lpvm-native::rt_jit
                              │
                              │ build JitBuffer + linked.entries (name → offset)
                              │ derive size from sorted-offset deltas
                              │
                              ▼
        lp_perf::emit_jit_map_load(base, len, &[JitSymbolEntry])
                              │
                              │ sink dispatch (Cargo feature on lp-perf)
                              │
                ┌─────────────┼──────────────┐
                ▼             ▼              ▼
            syscall          log           noop
            (RV32 fw)     (host dev)     (default)
                │
                │ ecall #SYSCALL_JIT_MAP_LOAD
                │ a0=base a1=len a2=count a3=entries_ptr
                ▼
   ─────────────┼─────────────────────────────────────────
                │                  HOST (lp-riscv-emu)
                ▼
     run_loops.rs::handle_syscall
     (SYSCALL_JIT_MAP_LOAD branch)
                │
                │ read entries[] from guest memory,
                │ resolve name_ptr/name_len strings
                ▼
     ProfileSession::on_jit_map_load(base, &[entries])
                │
                ▼
     JitSymbols.add_module(base, entries, cycle_count)
        Vec<JitSymbolRecord {
          base_addr, offset, size, name,
          loaded_at_cycle, unloaded_at_cycle: None
        }>
                │
                │ ... at session end ...
                ▼
     ProfileSession::finish_with_symbolizer
        ├── re-read meta.json from disk
        ├── splice in dynamic_symbols: [...]
        └── write meta.json back

   ──────────────────────────────────────────────────────
                              POST-RUN

      lp-cli profile reporting          lp-riscv-emu alloc heap-summary
                │                                   │
        loads meta.json                     loads meta.json
                │                                   │
                ▼                                   ▼
     Symbolizer::new(static, dynamic)    SymbolResolver::load (merges
        sorted intervals over both        static + dynamic into one
        — static wins on overlap          sorted interval list)
        — JIT region falls through
          to dynamic before <jit:0x...>
                │                                   │
                ▼                                   ▼
       report.txt, cpu-profile.json,        report.txt heap-summary
       speedscope.json                      with shader function names
       all show shader function names
```

## Main components

### `JitSymbolEntry` (ABI struct, `lp-riscv-emu-shared`)

```rust
#[repr(C)]
pub struct JitSymbolEntry {
    pub offset: u32,    // byte offset within the JIT module
    pub size: u32,      // function size in bytes
    pub name_ptr: u32,  // guest pointer to UTF-8 name
    pub name_len: u32,
}
```

Defined here (not in `lp-perf`) because the host syscall handler in
`lp-riscv-emu` and the guest emitter in `lp-perf::sinks::syscall`
must agree on the layout, and `lp-riscv-emu-shared` is the natural
seam.

### `lp-perf::emit_jit_map_load` (guest emission)

Mirrors the existing `emit_begin!` / `emit_end!` sink pattern.
Selected at compile time by the `syscall` / `log` / (none) feature
on `lp-perf`. fw-emu and fw-esp32 already set
`lp-perf/syscall`. No new feature plumbing through
`fw-emu → fw-core → lp-engine → lpvm-native`.

```rust
// in lp-base/lp-perf/src/lib.rs
#[cfg(feature = "syscall")]
pub use lp_riscv_emu_shared::JitSymbolEntry;

#[inline(always)]
pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry]) {
    sinks::emit_jit_map_load(base, len, entries);
}
```

The syscall sink does the ecall:

```rust
// in lp-base/lp-perf/src/sinks/syscall.rs (RV32 only)
pub fn emit_jit_map_load(base: u32, len: u32, entries: &[JitSymbolEntry]) {
    use lp_riscv_emu_shared::SYSCALL_JIT_MAP_LOAD;
    let count = entries.len() as i32;
    let entries_ptr = entries.as_ptr() as i32;
    unsafe {
        core::arch::asm!(
            "ecall",
            in("x17") SYSCALL_JIT_MAP_LOAD,
            in("x10") base as i32,
            in("x11") len as i32,
            in("x12") count,
            in("x13") entries_ptr,
            options(nostack, preserves_flags),
        );
    }
}
```

### `compile_module_jit` integration (`lpvm-native::rt_jit::compiler`)

After building `JitBuffer`, before returning, derive sizes from
sorted offsets and emit:

```rust
let buffer = JitBuffer::from_code(linked.code);
emit_jit_map(&buffer, &linked.entries);

fn emit_jit_map(buffer: &JitBuffer, entries: &BTreeMap<String, usize>) {
    // Sort by offset so we can derive size = next_offset - this_offset
    // (last function's size = buffer.len() - last_offset).
    let mut sorted: Vec<(&str, usize)> =
        entries.iter().map(|(n, o)| (n.as_str(), *o)).collect();
    sorted.sort_by_key(|&(_, o)| o);

    let total = buffer.len();
    let jit_entries: Vec<JitSymbolEntry> = sorted
        .iter()
        .enumerate()
        .map(|(i, &(name, off))| {
            let next = sorted.get(i + 1).map(|&(_, o)| o).unwrap_or(total);
            JitSymbolEntry {
                offset: off as u32,
                size: (next - off) as u32,
                name_ptr: name.as_ptr() as u32,
                name_len: name.len() as u32,
            }
        })
        .collect();

    let base = unsafe { buffer.entry_ptr(0) } as u32;
    lp_perf::emit_jit_map_load(base, buffer.len() as u32, &jit_entries);
}
```

### `JitSymbols` overlay (`lp-riscv-emu/src/profile/jit_symbols.rs`)

```rust
pub struct JitSymbols {
    entries: Vec<JitSymbolRecord>,
}

pub struct JitSymbolRecord {
    pub base_addr: u32,
    pub offset: u32,
    pub size: u32,
    pub name: String,
    pub loaded_at_cycle: u64,
    pub unloaded_at_cycle: Option<u64>,  // always None in m5
}

impl JitSymbols {
    pub fn add_module(
        &mut self,
        base: u32,
        entries: &[(u32, u32, String)],  // (offset, size, name)
        cycle: u64,
    );

    pub fn lookup(&self, pc: u32) -> Option<&str>;  // linear scan, latest-wins on overlap

    pub fn to_json(&self) -> serde_json::Value;     // [{addr, size, name, loaded_at_cycle, unloaded_at_cycle}]
}
```

The on-disk shape:

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

Both cycle fields ride along even though m5's lookups ignore them.
Future timeline-aware lookup / UNLOAD wiring lands without a
schema migration.

### `ProfileSession` integration (`lp-riscv-emu/src/profile/mod.rs`)

- New field: `jit_symbols: JitSymbols`.
- New method: `on_jit_map_load(&mut self, base: u32, entries: &[...], cycle: u64)`
  called by the syscall handler.
- `finish_with_symbolizer` reads `meta.json` back from disk,
  splices in `dynamic_symbols`, writes it back. Single
  re-serialize at finish; cost dominated by the report.txt build
  that already happens here.

### Syscall handler (`lp-riscv-emu/src/emu/emulator/run_loops.rs`)

Two new branches in `handle_syscall`, both gated on
`#[cfg(feature = "std")]` like the existing perf-event and
alloc-trace branches:

- `SYSCALL_JIT_MAP_LOAD`: read `(base, len, count, entries_ptr)` from
  args, walk `entries_ptr..entries_ptr + count*16` reading each
  `JitSymbolEntry`, resolve `name_ptr`/`name_len` to a `String`,
  call `session.on_jit_map_load(...)`.
- `SYSCALL_JIT_MAP_UNLOAD`: `log::error!("SYSCALL_JIT_MAP_UNLOAD
  not yet implemented; symbols may be stale"); a0 = 0;
  StepResult::Continue`.

### CLI symbolizer (`lp-cli/src/commands/profile/symbolize.rs`)

`Symbolizer::new` widened to take static + dynamic symbol slices.
Internally builds **two** sorted-interval tables. Lookup:

1. Try static (interval lookup over ELF symbols).
2. On miss, try dynamic (interval lookup over JIT symbols).
3. On miss, fall back to `<jit:0x...>` for `pc >= 0x8000_0000` or
   `<unknown:0x...>` otherwise.

Static wins on overlap (static is canonical; collisions shouldn't
happen but the order makes the policy explicit).

### Alloc-trace `SymbolResolver` (`lp-riscv-emu/src/profile/alloc.rs`)

Existing `TraceMetaSymbols` deserializer extended:

```rust
#[derive(Debug, Deserialize)]
struct TraceMetaSymbols {
    symbols: Vec<SymbolEntry>,
    #[serde(default)]
    dynamic_symbols: Vec<SymbolEntry>,  // NEW; ignores cycle fields here
}
```

`SymbolResolver::load` merges both arrays into the same sorted
interval `Vec<(addr, end, full, display)>`. Static-first ordering
preserved by inserting static entries before dynamic entries with
the same addr (shouldn't happen in practice).
