# Milestone 4: Hardware Perf-Log + Emu/Device Correlation

## Goal

Make the perf-event system cross-platform: same engine code, same
event vocabulary, emitted from real ESP32-C6 hardware via console with
real cycle-counter timecodes. Ingest device output back into the same
trace dir shape that emu produces. Add a four-corner correlation
report that quantifies *how well emu cycle-deltas predict device
cycle-deltas* — the validation story for the entire profiler.

This is the milestone that turns the profiler from "trustworthy on
emu" into "trustworthy as a proxy for device perf."

## Suggested Plan Name

`profile-m4-hardware-correlation`

## Scope

### In scope

- **`HardwarePerfSink` for `fw-esp32`** in new file
  `lp-fw/fw-esp32/src/perf_sink.rs`. Implements `PerfEventSink` from
  `lp-engine`. Emits events as console lines:

  ```
  [perf] 1234567890 frame B
  [perf] 1234567892 shader-link B
  [perf] 1234571234 shader-link E
  ```

  Format: `[perf] <cycles> <name> <kind>` where `<cycles>` is read
  from the platform's monotonic cycle counter (`mcycle` CSR on
  ESP32-C6, accessed via existing platform helpers if present, or
  via a new minimal helper). `<kind>` is `B` / `E` / `I` matching the
  `events.jsonl` schema.

  Wired into `fw-esp32` engine init the same way `EmuPerfSink` was
  wired into `fw-emu` in m1 — minimal, mirror the existing pattern.

- **`HardwarePerfSink` abstraction in `lp-engine`.** The `lp-engine`
  side likely needs a clock source abstraction (`fn now_cycles() ->
  u64`) so the sink can be platform-agnostic. ESP32-C6 reads
  `mcycle`; future ports (other boards) implement their own. Defined
  in `lp-engine`; instantiated in `fw-esp32`.

- **`lp-cli profile capture` subcommand.** Captures device console
  output during a session into a device trace dir.

  ```
  lp-cli profile capture <serial-port> [--baud 115200]
                                       [--workload-name STR=unknown]
                                       [--mode-name STR=unknown]
                                       [--note STR]
                                       [--max-secs N=30]
                                       [--max-events N=100000]
  ```

  Reads from serial port until either `--max-secs`, `--max-events`,
  or the device emits a known terminator event (e.g.
  `frame E` after expected count for whatever the firmware is
  configured to do). Writes to a device trace dir in the same shape
  as emu trace dirs. The user is responsible for ensuring the device
  is running the workload of interest; m4 doesn't drive the device,
  only listens.

- **Console parser** in new module
  `lp-cli/src/commands/profile/parse_console.rs`. Reads lines of
  format `[perf] <cycles> <name> <kind>`, produces `PerfEvent`
  records. Lines not matching the prefix are passed through to
  stdout (so users can see device debug output during capture).
  Parser is robust to interleaved log noise.

- **Device trace dir shape.** Same convention as emu, but with
  fewer files (no `cpu-profile.json`, no `cpu-profile.speedscope.json`):

  ```
  traces/<timestamp>--<workload-name>--<mode-name>--device[--<note>]/
    meta.json                # clock_source="esp32c6_mcycle", source="device", port info
    events.jsonl             # parsed perf events from device
    raw-console.log          # unredacted console output for forensic use
    report.txt               # event timeline summary
  ```

  `--device` in the dir name distinguishes from emu runs at a glance
  and prevents `--diff` from auto-pairing emu with device.

- **`perf-log-diff` subcommand mode.** Diffs two trace dirs using only
  `events.jsonl` (works for device-vs-device, emu-vs-emu when CPU
  data isn't present, or as a sub-section of a full diff).

  ```
  lp-cli profile diff <a> <b> [--perf-log-only]
  ```

  When `--perf-log-only` set OR when neither dir has `cpu-profile.json`,
  diff falls back to event-pair durations:

  ```
  Perf-log diff (a → b)
  =====================
    clock_source:   esp32c6_mcycle → esp32c6_mcycle
    
    Per-event-pair duration (median across instances):
       frame                100,234 → 92,114 cycles  (-8.1%)
       shader-link        2,400,000 → 2,400,000 cycles  (no change)
       project-load     145,000,000 → 144,200,000 cycles  (-0.6%)
  ```

  Event pairs identified by matching adjacent Begin/End with same
  name. Multiple instances → median; standard deviation surfaced
  when significant.

- **Four-corner correlation report.** New subcommand:

  ```
  lp-cli profile correlate --emu-a <dir> --emu-b <dir>
                           --device-a <dir> --device-b <dir>
  ```

  Computes three diffs (emu A→B, device A→B, and the cross-comparison)
  and prints a table:

  ```
  Emu/Device Correlation
  ======================
  Workload: examples-basic, Mode: steady-render
  
  Per event boundary:
                       Emu Δ%     Device Δ%   Agreement
    frame              -8.0%      -7.2%       within tolerance ✓
    shader-link         0.0%       0.0%       within tolerance ✓
    palette_warm      -18.2%     -22.1%       direction match, magnitude off
  
  Summary:
    Mean abs. correlation:  0.94 (excellent)
    Sign agreement:         9/10 (90%)
    Mean magnitude error:   ±2.4 percentage points
  
    Cycle model under-predicts device deltas by 1.8pp on average.
    Recommend: investigate Andes N22 BranchTaken cost
              (currently 2; suggested 3 based on this dataset).
  ```

  "Agreement" classification:
  - within tolerance: |emu Δ% − device Δ%| < 3pp
  - direction match: same sign, magnitude differs >= 3pp
  - inversion: opposite sign (worth investigating)
  - non-comparable: missing in one side

  Cycle-model recommendations are *advisory* — printed but never
  applied automatically. Refinements happen via explicit `cycle_model.rs`
  edits, validated by re-running correlation.

- **Cycle-model refinement (data-driven).** If correlation shows a
  clear systematic bias, m4 includes one optional pass of cost-class
  tuning in `lp-riscv-emu/src/emu/cycle_model.rs` to reduce the bias.
  Captured as a separate commit so the before/after correlation deltas
  are clear. *Only* applied if the correlation report shows it's
  warranted; not speculative.

- **Tests.**
  - Unit test for console parser: lines with various
    well-formed and malformed shapes, interleaved with non-perf
    output.
  - Unit test for `perf-log-diff`: synthetic `events.jsonl` pairs.
  - Unit test for correlation computation: hand-built four-corner
    fixture → expected agreement table.
  - Integration test (gated, requires hardware): manual run of
    `profile capture` against a real ESP32-C6, verify trace dir
    produces.

### Out of scope

- Driving the device automatically (flashing, starting workload).
  User runs the firmware manually; `lp-cli profile capture` just
  listens.
- Other device targets beyond `fw-esp32` (other boards add their own
  `HardwarePerfSink` impls following the same pattern).
- Wireless capture (USB/serial only).
- JIT symbol overlay — m5. (Device-side symbolization comes for free
  when m5 lands, since the perf events don't carry PCs.)
- Real-time live streaming view of device events ("perf top" style).

## Key Decisions

- **Console output, not a dedicated wire protocol.** The ESP32-C6
  already has a serial console for logging. Layering perf events over
  the same channel costs zero hardware setup and leverages existing
  `[perf]` filtering. The format is intentionally human-grep-able
  for debugging.

- **`mcycle` CSR as the device timestamp source.** Direct CPU-cycle
  count, monotonic, no driver needed. Wraps in 2^64 cycles which is
  decades.

- **Device trace dirs marked `--device` in the name.** Prevents emu
  diffs from auto-pairing with device runs (which would be apples to
  oranges in absolute cycle counts even if deltas correlate well).

- **Correlation is advisory.** The report suggests refinements; it
  never auto-applies them. Cycle-model changes are deliberate, peer-
  reviewed edits.

- **Per-event-pair median across instances.** A `frame` event pair
  fires many times; the median is what we compare. Standard deviation
  surfaced when it's high enough to matter.

- **Capture is one-shot, not interactive.** `lp-cli profile capture`
  reads serial until a stop condition, writes the trace dir, exits.
  Future iteration could add interactive UIs; m4 is the minimum
  useful version.

- **`HardwarePerfSink`'s clock source is platform-defined.**
  `lp-engine` defines the abstraction; each port chooses what to
  read. ESP32-C6 reads `mcycle`. Other ports implement to taste.

## Deliverables

### `lp-engine` crate
- New: `lp-engine/src/perf.rs` updates — clock-source abstraction
  (`pub trait CycleClock { fn now(&self) -> u64; }` or similar).
  `PerfEventSink` interface unchanged, gains constructor that takes
  a clock.

### `fw-esp32` crate
- New: `lp-fw/fw-esp32/src/perf_sink.rs` — `HardwarePerfSink`
  implementation reading `mcycle`.
- Updated: `lp-fw/fw-esp32/src/main.rs` — wires `HardwarePerfSink`
  into engine init.

### `lp-riscv-emu` crate
- Possibly: refined `CycleModel::Esp32C6` cost values *if* m4's
  correlation work shows clear bias. As separate commit.

### `lp-cli` crate
- New: `lp-cli/src/commands/profile/parse_console.rs` — console
  parser.
- New: `lp-cli/src/commands/profile/capture.rs` — `profile capture`
  subcommand.
- New: `lp-cli/src/commands/profile/correlate.rs` — `profile
  correlate` subcommand.
- Updated: `lp-cli/src/commands/profile/diff.rs` — adds
  `--perf-log-only` mode; auto-detects when only `events.jsonl`
  available.
- Updated: `lp-cli/src/commands/profile/args.rs` — args for new
  subcommands.

### Tests
- `lp-cli/tests/profile_console_parser.rs` — parser unit tests.
- `lp-cli/tests/profile_correlate.rs` — correlation logic unit
  tests.
- `lp-cli/tests/profile_capture_smoke.rs` — capture subcommand
  smoke test (uses a mocked serial source, not hardware).

## Dependencies

- m0, m1, m2, m3 — full prior chain.
  - m1 for `events.jsonl` schema (this milestone *produces* device-side
    `events.jsonl` of the same shape).
  - m3 for diff machinery (`perf-log-diff` extends it).

## Validation

```bash
# Workspace builds
cargo build --workspace

# fw-esp32 builds with HardwarePerfSink
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server

# Unit tests
cargo test -p lp-cli

# Manual hardware capture
# Step 1: flash fw-esp32 to a board running examples/basic
# Step 2: capture
cargo run -p lp-cli --release -- profile capture /dev/cu.usbserial-XXX \
  --workload-name examples-basic --mode-name steady-render \
  --max-secs 10
# Verify: traces/<sess>--device/events.jsonl populated with [perf]
# events; raw-console.log preserved.

# Perf-log diff between two device runs
# (capture twice with different code states)
cargo run -p lp-cli --release -- profile diff \
  traces/<device-a>--device traces/<device-b>--device --perf-log-only
# Verify: per-event-pair duration deltas printed.

# Four-corner correlation
# Need: emu-a, emu-b (different code states), device-a, device-b
cargo run -p lp-cli --release -- profile correlate \
  --emu-a traces/<emu-a> --emu-b traces/<emu-b> \
  --device-a traces/<dev-a>--device --device-b traces/<dev-b>--device
# Verify: agreement table printed; sign-agreement metric reasonable.

# m3 regression: emu-only diff still works
cargo run -p lp-cli --release -- profile diff \
  traces/<emu-a> traces/<emu-b>
# Should produce CPU diff as before.
```

## Estimated Scope

- New code: ~700-1000 LOC.
  - `HardwarePerfSink` (fw-esp32 + clock abstraction in lp-engine):
    ~150-200.
  - Console parser: ~100-150.
  - Capture subcommand: ~150-200.
  - Correlate subcommand + computation: ~200-300.
  - Diff `--perf-log-only` mode: ~100-150.
- Tests: ~300-500 LOC.
- Optional cycle-model tuning commit: small (numbers only).
- Files touched: ~10-15.

## Agent Execution Notes

Implementation order:

1. Read `fw-esp32`'s engine init path to understand where to inject
   the sink.
2. Read or implement an `mcycle` CSR helper in `fw-esp32` (search
   first; may already exist for panic-handler timing).
3. Add clock-source abstraction in `lp-engine` if needed.
4. Implement `HardwarePerfSink` in `fw-esp32`. Smoke-test by
   flashing and observing `[perf]` lines on the console.
5. Implement console parser in `lp-cli`. Heavy unit tests with
   real captured logs as fixtures (capture a small session manually
   first, save lines as test inputs).
6. Implement `profile capture` subcommand. Smoke-test against a
   real device.
7. Implement `perf-log-diff` mode in `diff.rs`. Test with two
   device captures.
8. Implement `profile correlate` subcommand. Test with a four-corner
   fixture.
9. Run a real correlation pass: capture emu and device for two real
   code states. Inspect the agreement report.
10. If the correlation shows clear systematic bias in
    `CycleModel::Esp32C6`, make a tuning commit to reduce it.
    Re-run correlation to verify improvement.
