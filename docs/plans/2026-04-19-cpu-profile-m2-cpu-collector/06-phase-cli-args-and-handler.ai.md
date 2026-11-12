# Phase 6 — CLI: `--cycle-model` + `--collect cpu` wiring

Add the `--cycle-model {esp32c6,uniform}` flag to `args.rs`. Flip
`--collect`'s default from `events` (m1) to `cpu`. Implement the
auto-include-events rule: any `--collect` containing `cpu` also
implicitly enables `events`. Add `cpu` as a recognized collector
name in handler.rs's collector validation. Plumb the cycle-model
choice through to `Riscv32Emulator::with_cycle_model`. Add a
`cycle_model` field to `SessionMetadata` and populate it.

This phase **does not yet write** `cpu-profile.json` or
`cpu-profile.speedscope.json` — that's P7. P6 lands the wiring
that lets P7 pull the `CpuCollector` out of the session.

**Manual review** — extends m1's CLI shape and `SessionMetadata`.

## Dependencies

- **P4** — needs `CpuCollector` type to instantiate.
- **m1 fully merged** — needs m1's `args.rs`, `handler.rs`,
  `mode/` module, `SessionMetadata`, and `--collect` validation
  pattern.

## Files

### `lp-cli/src/commands/profile/args.rs`

Two changes:
1. Flip `--collect` default from `"events"` to `"cpu"`.
2. Add `--cycle-model` flag.

```rust
#[derive(clap::Args, Debug)]
pub struct ProfileArgs {
    #[arg(default_value = "examples/basic")]
    pub dir: PathBuf,

    /// Comma-separated collectors. Recognized: cpu, events, alloc.
    /// `events` is auto-included when `cpu` is enabled.
    #[arg(long, value_delimiter = ',', default_value = "cpu")]              // [m2: was "events"]
    pub collect: Vec<String>,

    #[arg(long, value_enum, default_value_t = ProfileMode::SteadyRender)]
    pub mode: ProfileMode,

    /// Cycle attribution model for the CPU collector.
    #[arg(long, value_enum, default_value_t = CycleModelArg::Esp32C6)]      // [m2 NEW]
    pub cycle_model: CycleModelArg,

    #[arg(long, default_value_t = 200_000_000)]
    pub max_cycles: u64,

    #[arg(long)]
    pub note: Option<String>,
}

#[derive(clap::ValueEnum, Clone, Copy, Debug)]                               // [m2 NEW]
pub enum CycleModelArg { Esp32C6, Uniform }

impl CycleModelArg {
    pub fn label(self) -> &'static str {
        match self {
            Self::Esp32C6 => "esp32c6",
            Self::Uniform => "uniform",
        }
    }
    pub fn to_emu(self) -> lp_riscv_emu::CycleModel {
        match self {
            Self::Esp32C6 => lp_riscv_emu::CycleModel::Esp32C6,
            Self::Uniform => lp_riscv_emu::CycleModel::InstructionCount,
        }
    }
}
```

### `lp-cli/src/commands/profile/handler.rs`

Three changes:

#### 1. Cycle model plumbing

```rust
let cycle_model = args.cycle_model.to_emu();
let mut emu = Riscv32Emulator::new(...)
    .with_cycle_model(cycle_model);
```

#### 2. Collector list — `cpu` validation + events auto-include

```rust
// Resolve the requested collector set.
let mut requested: Vec<&str> = args.collect.iter().map(String::as_str).collect();
let wants_cpu = requested.iter().any(|c| *c == "cpu");
if wants_cpu && !requested.iter().any(|c| *c == "events") {
    requested.push("events");
}

let mut collectors: Vec<Box<dyn Collector>> = Vec::new();
for name in &requested {
    match *name {
        "events" => collectors.push(Box::new(EventsCollector::new(events_path.clone()))),
        "alloc"  => collectors.push(Box::new(AllocCollector::new(heap_trace_path.clone()))),
        "cpu"    => collectors.push(Box::new(CpuCollector::new(args.cycle_model.label()))),
        other    => bail!("unknown collector: {other}"),
    }
}
```

#### 3. `SessionMetadata.cycle_model` population

(Field added in `output.rs` below.)

```rust
let meta = SessionMetadata {
    workload: args.dir.file_name()...,
    mode: args.mode.label().to_string(),
    cycle_model: args.cycle_model.label().to_string(),  // [m2 NEW]
    max_cycles: args.max_cycles,
    cycles_used: emu.cycle_count(),
    terminated_by: ...,
    note: args.note.clone(),
    symbols: trace_symbols.clone(),
};
```

### `lp-cli/src/commands/profile/output.rs` (m1 file)

Add `cycle_model: String` to `SessionMetadata`:

```rust
#[derive(Serialize)]
pub struct SessionMetadata {
    pub workload: String,
    pub mode: String,
    pub cycle_model: String,                  // [m2 NEW]
    pub max_cycles: u64,
    pub cycles_used: u64,
    pub terminated_by: String,
    pub note: Option<String>,
    pub symbols: Vec<TraceSymbol>,
}
```

Schema version stays `1` (m1 Q9 policy: nothing real consumes
meta.json yet).

## Tests

### `lp-cli/src/commands/profile/args.rs#tests`

```rust
#[test]
fn default_collect_is_cpu() {
    let args = ProfileArgs::parse_from(&["lp-cli", "profile", "examples/basic"]);
    assert_eq!(args.collect, vec!["cpu".to_string()]);
}

#[test]
fn default_cycle_model_is_esp32c6() {
    let args = ProfileArgs::parse_from(&["lp-cli", "profile", "examples/basic"]);
    assert!(matches!(args.cycle_model, CycleModelArg::Esp32C6));
}

#[test]
fn cycle_model_uniform_parses() {
    let args = ProfileArgs::parse_from(&["lp-cli", "profile", "examples/basic",
                                          "--cycle-model", "uniform"]);
    assert!(matches!(args.cycle_model, CycleModelArg::Uniform));
}
```

### `lp-cli/src/commands/profile/handler.rs#tests`

If handler.rs has helpers that take args without running the full
binary, add:

```rust
#[test]
fn cpu_auto_includes_events() {
    // resolve_collectors(args with collect=["cpu"]) → vec!["cpu", "events"]
}

#[test]
fn alloc_alone_does_not_auto_include_events() {
    // resolve_collectors(args with collect=["alloc"]) → vec!["alloc"]
}

#[test]
fn unknown_collector_errors() {
    // resolve_collectors(args with collect=["bogus"]) → Err
}
```

(If the handler doesn't expose a testable helper, factor
`resolve_collectors(args) -> Result<Vec<&'static str>>` out of
`handler::run` for testability.)

### `lp-cli/tests/profile_smoke.rs`

If m1 has a smoke test that runs `lp-cli profile examples/basic`
end-to-end, ensure it passes with the new defaults. The test should
expect `cpu-profile.json` and `cpu-profile.speedscope.json` to exist
— but those don't exist until P7. Until P7 lands, P6's smoke test
expects only `meta.json`, `events.jsonl`, `report.txt` (the cpu
collector runs but writes nothing to disk yet).

(P7 will tighten the smoke test to also expect cpu-profile files.)

## Risk + rollout

- **Risk**: m1's smoke test asserts `--collect events` produces
  exactly `events.jsonl`. After P6's default flip, the same command
  with no `--collect` flag now produces `events.jsonl` *and* runs
  the cpu collector (which is silent at this phase). m1's smoke
  test should still pass because cpu's silence at P6 doesn't add
  unexpected files. Verify before submitting.
- **Risk**: `SessionMetadata` field addition. If m1's `meta.json`
  test does exact-shape JSON matching, it'll fail. Update m1's test
  to include the new field, or have the test do partial-match. (m1
  Q9 already says schema_version stays 1; extending the JSON shape
  is fine.)
- **Risk**: collector ordering. The fan-out order in
  `dispatch_instruction` and `on_perf_event` follows registration
  order. With `requested.push("events")` the events collector ends
  up *after* cpu. This means `EventsCollector` writes the
  `profile:start` event to events.jsonl *after* the gate fires
  Enable on cpu — fine because they're independent observers, not
  ordered consumers.
- **Rollback**: revert the args.rs default flip, the handler
  collector match, and the SessionMetadata field. m1 behavior
  restored.

## Acceptance

- `cargo build -p lp-cli` succeeds.
- `cargo test -p lp-cli` passes.
- `lp-cli profile examples/basic --collect cpu --max-cycles 1000000`
  runs to completion (no panic, exit 0). Trace dir contains
  `meta.json` (with `cycle_model: "esp32c6"`), `events.jsonl`,
  `report.txt`. (No cpu-profile files yet — P7.)
- `lp-cli profile examples/basic --collect cpu --cycle-model uniform
  --max-cycles 1000000` produces `meta.json` with
  `cycle_model: "uniform"`.
