# Phase 7 — Symbolizer + Speedscope writer + canonical JSON writer + report symbolization

Add `lp-cli/src/commands/profile/symbolize.rs` (PC → name resolver
with interval lookup). Add `output_speedscope.rs` (Speedscope
"evented" JSON writer using synthetic per-edge events). Add
`output_cpu_json.rs` (canonical schema-versioned JSON for m3's diff).
Extend `output.rs` to write a symbolized "CPU summary" section to
`report.txt` (replacing/supplementing CpuCollector's default raw-PC
output).

This is the largest phase by file count but the most pure-function
code in the plan — every writer is a `(input data) → bytes` function
with deterministic output.

**Sub-agent suitable**: yes (multiple new files, each with its own
tests, no cross-file invariants beyond the schemas).

## Dependencies

- **P4** — needs `CpuCollector` data model (`func_stats`,
  `call_edges`, `total_cycles_attributed`, `cycle_model_label`).
- **P6** — needs handler.rs hookup point (the place where output
  writers are called at session finish).

## Files

### `lp-cli/src/commands/profile/symbolize.rs` — new

```rust
use std::borrow::Cow;
use lp_riscv_emu::profile::TraceSymbol;

pub struct Symbolizer<'a> {
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
                return Cow::Borrowed(name);
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

Tests in `symbolize.rs#tests`:

```rust
fn fixture() -> Vec<TraceSymbol> { vec![
    TraceSymbol { addr: 0x1000, size: 0x40, name: "alpha".into() },
    TraceSymbol { addr: 0x1100, size: 0x80, name: "beta".into() },
    TraceSymbol { addr: 0x2000, size: 0x10, name: "gamma".into() },
] }

#[test] fn pc_zero_is_root() { /* assert "<root>" */ }
#[test] fn exact_addr_hit() { /* lookup(0x1000) == "alpha" */ }
#[test] fn last_byte_hit() { /* lookup(0x103f) == "alpha" */ }
#[test] fn one_past_end_misses() { /* lookup(0x1040) is unknown */ }
#[test] fn between_symbols_misses() { /* lookup(0x1080) is unknown */ }
#[test] fn ram_pc_is_jit() { /* lookup(0x80000000 + 0x1234) == "<jit:0x80001234>" */ }
#[test] fn rom_pc_is_unknown() { /* lookup(0x500) == "<unknown:0x00000500>" */ }
#[test] fn boundary_addr_plus_size_minus_one() { /* hit */ }
```

### `lp-cli/src/commands/profile/output_speedscope.rs` — new

```rust
//! Speedscope "evented" profile writer for the CpuCollector.
//!
//! NOTE: events are SYNTHETIC, fabricated from aggregated call_edges
//! at finish time (R13/R15). The flame chart is shape-correct
//! (every callee bar has the right cumulative width relative to its
//! parent) but the x-axis is "synthetic cycles", not wall-clock.
//! Multiple non-contiguous calls to the same function smash together
//! into one bar. For real chronological events, use the cpu-log
//! collector (m6 future work).
```

Algorithm:

1. **Frame interning**: collect every PC appearing in `func_stats`
   keys, `call_edges` keys (caller and callee). Symbolize each. Build
   `frames: Vec<{ name: String }>` and `pc_to_frame: HashMap<u32, usize>`.

2. **Event generation** — DFS from `<root>` (PC = 0):
   ```rust
   fn emit_events(caller: u32, edges: &HashMap<(u32,u32), CallEdge>,
                  pc_to_frame: &HashMap<u32, usize>,
                  cursor: &mut u64,
                  events: &mut Vec<Event>) {
       // Find all (caller, callee) edges; iterate sorted by callee for determinism
       let mut callees: Vec<(u32, &CallEdge)> = edges.iter()
           .filter_map(|((c, d), e)| if *c == caller { Some((*d, e)) } else { None })
           .collect();
       callees.sort_by_key(|(d, _)| *d);

       for (callee, edge) in callees {
           let frame = pc_to_frame[&callee];
           events.push(Event::Open { frame, at: *cursor });
           // Spend the inclusive cycles: split into recursive children + own self time
           let before = *cursor;
           emit_events(callee, edges, pc_to_frame, cursor, events);
           let recursed = *cursor - before;
           let self_time = edge.inclusive_cycles.saturating_sub(recursed);
           *cursor += self_time;
           events.push(Event::Close { frame, at: *cursor });
       }
   }
   ```

3. **Wrapper JSON**:
   ```json
   {
     "$schema": "https://www.speedscope.app/file-format-schema.json",
     "exporter": "lp-cli profile m2",
     "name": "<workload-name>",
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

Public API:

```rust
pub fn write(cpu: &CpuCollector, symbols: &Symbolizer<'_>,
             workload: &str, mode: &str, dest: &Path) -> std::io::Result<()> {
    let json = build(cpu, symbols, workload, mode);
    std::fs::write(dest, serde_json::to_vec_pretty(&json)?)?;
    Ok(())
}

fn build(cpu: &CpuCollector, symbols: &Symbolizer<'_>,
         workload: &str, mode: &str) -> serde_json::Value { ... }
```

`build` is the testable seam.

Tests in `output_speedscope.rs#tests`:

```rust
#[test]
fn smoke_empty_cpu_produces_valid_envelope() { /* … */ }
#[test]
fn three_function_call_graph_produces_correct_event_count() {
    // Build a CpuCollector by hand with 3 funcs and 3 edges.
    // build(...) should produce 6 events (3 open, 3 close).
}
#[test]
fn json_parses_back_with_speedscope_fields() {
    let json = build(...);
    assert_eq!(json["$schema"], "https://www.speedscope.app/file-format-schema.json");
    assert_eq!(json["profiles"][0]["type"], "evented");
}
#[test]
fn end_value_matches_total_attributed_cycles() { /* … */ }
```

### `lp-cli/src/commands/profile/output_cpu_json.rs` — new

Schema:

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
      "caller_name": "render::frame",
      "callee": "0x80005678",
      "callee_name": "shader::palette_warm",
      "count": 1024,
      "inclusive_cycles": 4700000
    }
  ]
}
```

Public API:

```rust
pub fn write(cpu: &CpuCollector, symbols: &Symbolizer<'_>,
             dest: &Path) -> std::io::Result<()> { ... }
fn build(cpu: &CpuCollector, symbols: &Symbolizer<'_>) -> serde_json::Value { ... }
```

PC formatting: `format!("0x{:08x}", pc)` everywhere (R12).

Tests in `output_cpu_json.rs#tests`:

```rust
#[test]
fn schema_version_is_1() { /* … */ }
#[test]
fn cycle_model_label_round_trips() { /* … */ }
#[test]
fn func_stats_keys_are_lowercase_8_hex() {
    // regex: ^0x[0-9a-f]{8}$ for every key in func_stats
}
#[test]
fn call_edges_includes_both_pcs_and_names() { /* … */ }
#[test]
fn round_trips_through_serde() {
    let v = build(...);
    let s = serde_json::to_string(&v).unwrap();
    let v2: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(v, v2);
}
```

### `lp-cli/src/commands/profile/output.rs` — symbolized CPU summary

Add a function that writes the top-N tables with symbol names:

```rust
pub fn write_cpu_summary_symbolized(
    w: &mut dyn Write,
    cpu: &CpuCollector,
    symbols: &Symbolizer<'_>,
) -> io::Result<()> {
    writeln!(w, "=== CPU summary ===")?;
    writeln!(w, "cycle_model={}, total_attributed_cycles={}",
             cpu.cycle_model_label, cpu.total_cycles_attributed)?;
    writeln!(w)?;
    writeln!(w, "Top 20 by self cycles:")?;
    let mut by_self: Vec<_> = cpu.func_stats.iter().collect();
    by_self.sort_by_key(|(_, s)| std::cmp::Reverse(s.self_cycles));
    for (pc, stats) in by_self.iter().take(20) {
        writeln!(w, "  {:>12}  {:>5.1}%  {}", stats.self_cycles,
                 percent_of_total(stats.self_cycles, cpu.total_cycles_attributed),
                 symbols.lookup(**pc))?;
    }
    writeln!(w)?;
    writeln!(w, "Top 20 by inclusive cycles:")?;
    // ... same shape with inclusive_cycles ...
    Ok(())
}
```

`CpuCollector::report_section` (the trait method from P4) keeps its
default raw-PC behavior — useful for non-symbolizing callers. The CLI
suppresses it and calls `write_cpu_summary_symbolized` instead.

Suppression mechanism: in `output::write_report`, special-case
`CpuCollector` in the per-collector loop:

```rust
for c in session.collectors.iter() {
    if let Some(cpu) = c.as_any().downcast_ref::<CpuCollector>() {
        write_cpu_summary_symbolized(&mut report_file, cpu, symbols)?;
    } else {
        c.report_section(&mut report_file)?;
    }
}
```

### `lp-cli/src/commands/profile/handler.rs` — hookup at finish

In the session-finish path:

```rust
let session = emu.finish_profile_session().expect("session present");

// Build symbolizer from the symbol list collected during ELF load.
let symbolizer = Symbolizer::new(&trace_symbols);

// Drain collectors and write outputs.
session.finish(&FinishCtx { dir: &dir })?;

// Cpu-specific outputs (cpu collector in the session).
if let Some(cpu) = session.collectors.iter()
    .find_map(|c| c.as_any().downcast_ref::<CpuCollector>())
{
    output_speedscope::write(
        cpu, &symbolizer,
        &workload_name, args.mode.label(),
        &dir.join("cpu-profile.speedscope.json"),
    )?;
    output_cpu_json::write(
        cpu, &symbolizer,
        &dir.join("cpu-profile.json"),
    )?;
}

// Report (cpu summary now symbolized; see output.rs change).
output::write_report(&mut report_file, &session, Some(&symbolizer))?;
```

`write_report` gains an `Option<&Symbolizer>` parameter; passes it
to the cpu-section special case above.

### `lp-cli/src/commands/profile/mod.rs`

Register the new submodules:
```rust
mod symbolize;
mod output_speedscope;
mod output_cpu_json;
```

## Risk + rollout

- **Risk**: Speedscope DFS termination. If `call_edges` somehow
  contains a cycle (e.g., recursion), the DFS recurses forever.
  Guard with a visited-set keyed by edge `(caller, callee)`. On
  re-visit, emit a single open/close pair without recursing.
  Document the trade-off: recursive functions appear flat. This is
  acceptable for m2; m6's cpu-log doesn't have this limitation.
- **Risk**: `pc_to_frame` indexing assumes every PC in `call_edges`
  also appears in `func_stats`. Verify by walking edges first and
  inserting frame entries for any missing PC.
- **Risk**: `as_any` downcast requires the `Any` bound on
  `Collector` (added in P3). If P3 didn't add it, P7 must.
- **Rollback**: delete the three new files and revert the handler
  hookup. m1+P3+P4+P5+P6 still produce a working tool that emits
  meta.json, events.jsonl, report.txt (no cpu-profile files).

## Acceptance

- `cargo build -p lp-cli` succeeds.
- `cargo test -p lp-cli` passes (all new symbolize/speedscope/json
  tests).
- `lp-cli profile examples/basic --collect cpu --max-cycles 5000000`
  produces all four cpu-related files: `meta.json`, `events.jsonl`,
  `cpu-profile.json` (parses as JSON, has schema_version 1),
  `cpu-profile.speedscope.json` (parses as JSON, has the speedscope
  envelope), `report.txt` (contains "CPU summary" section with
  symbol names not raw PCs).
- Drop the speedscope file into https://www.speedscope.app/ and
  visually confirm a flame chart renders.
