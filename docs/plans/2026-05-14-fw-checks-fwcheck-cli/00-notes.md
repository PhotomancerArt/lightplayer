# Firmware Checks + `lp-cli fwcheck` Notes

## Scope of work

Create a small shared firmware-check layer and a Rust-native `lp-cli` command for running hardware checks, starting with ESP32-C6 and the incremental shader compile stress check.

Primary goals:
- avoid Python and fragile shell process control
- centralize serial-port detection, build/run/capture behavior, and trace output
- centralize firmware check names, features, markers, and trace/report defaults
- make non-hardware-specific checks easier to share with `fw-emu` later
- keep firmware builds lean through feature flags

Out of scope for the first pass:
- making every existing ESP32 test portable to every firmware target
- replacing all `just` recipes at once if doing so would churn too much
- designing a large test framework
- changing normal firmware server boot behavior

## Current state

### Existing ESP32 checks

`fw-esp32` currently exposes check-like firmware modes through Cargo features:

- `test_rmt`
- `test_dither`
- `test_gpio`
- `test_usb`
- `test_json`
- `test_oom`
- `test_msafluid`
- `test_fluid_demo`
- `test_jit_math_perf`
- `test_shader_compile_incremental`

Relevant files:
- `lp-fw/fw-esp32/Cargo.toml`
- `lp-fw/fw-esp32/src/main.rs`
- `lp-fw/fw-esp32/src/tests/*`

`fw-esp32/src/main.rs` has repeated `cfg(any(feature = ...))` lists and dispatches directly to each feature-selected check.

### Existing `just` recipes

Relevant recipes live around the ESP32 demo/test area in `justfile`:

- `demo-esp32c6-host`
- `demo-esp32c6-host-naga`
- `demo-esp32c6-standalone`
- `fwtest-rmt-esp32c6`
- `fwtest-dithering-esp32c6`
- `fwtest-json-esp32c6`
- `fwtest-oom-esp32c6`
- `fwtest-msafluid-esp32c6`
- `fwtest-fluid-demo-esp32c6`
- `fwtest-jit-math-perf-esp32c6`
- `fwtest-shader-compile-incremental-esp32c6`
- `fwtest-shader-compile-stress-trace-esp32c6`

Some recipes use Cargo runner behavior and some invoke `espflash` directly. The shader compile trace recipe previously used `scripts/run_until_marker.py`; this plan replaces that path with `lp-cli fwcheck`.

Problems observed:
- `espflash` interactive port selection fails when not attached to a terminal
- capture behavior can mangle serial newlines if raw monitor output is handled poorly
- trace paths and markers are duplicated in shell recipes
- recipes are harder to reuse across targets

### Replaced Python helper

`scripts/run_until_marker.py` used to:
- spawns a command
- tees stdout/stderr to console and a trace file
- stops when a marker appears

The user wants this Python dependency removed in favor of normal Rust/Cargo-world tools.

### Existing `lp-cli`

`lp-cli` already uses `clap`, `anyhow`, `serialport`, `chrono`, and `serde_json`.

Relevant files:
- `lp-cli/src/main.rs`
- `lp-cli/src/commands/mod.rs`
- `lp-cli/src/commands/profile/*`

`lp-cli profile` already creates timestamped output directories under `profiles/` and writes structured output. The hardware-check command should follow that spirit, but output under `traces/`.

### Existing firmware check output

The incremental shader compile check now prints human summary lines, but a more durable perf workflow would benefit from structured records.

Candidate direction:
- checks emit JSONL-style records for machine-readable measurements
- `lp-cli fwcheck` captures trace output and optionally parses check records into `report.txt`
- human log lines can remain for ordinary serial readability

## Proposed terminology

Use `FwCheck` as the shared concept.

- `FwCheck`: what to run
- `FwCheckTarget`: where to run it (`esp32c6`, later `fw-emu`)
- `FwCheckConfig`: static metadata for CLI/build/capture
- check module: reusable code for a particular check when possible
- runner: target-specific execution mechanism
- trace: captured console/device output
- report: parsed summary written by host-side tooling

## Open questions

### 1. Should `fw-checks` be one crate with a `std` feature, or two crates?

Context:
- firmware wants `no_std` shared names/config and possibly check code
- `lp-cli` wants `std` helpers for parsing JSONL/check output and generating reports
- splitting into two crates is cleaner at the dependency level but more structure than we need right now

Suggested answer:
- Use one crate: `lp-fw/fw-checks`.
- Default is `no_std + alloc` if needed.
- Add `std` feature for host-side parsing/report generation helpers.
- Revisit two crates only if the `std` boundary becomes messy.

### 2. Should the first implementation migrate all existing ESP32 checks into `fw-checks`?

Context:
- many checks are hardware-specific and not worth moving immediately
- `shader_compile_incremental` has reusable corpus/config/reporting, but ESP32-specific board init, heap stats, and cycle counter should remain target-owned

Suggested answer:
- No. Start by centralizing metadata for all named checks, and migrate shared code for only the shader compile stress check.
- Keep hardware-specific modules in `fw-esp32` until there is a clear reusable core.

### 3. What should the CLI command be called?

Context:
- user mentioned `fwcheck` as a likely command name
- checks may also run on `fw-emu`, so the name is slightly hardware-biased
- but `fwcheck esp32c6 ...` is clear and practical

Suggested answer:
- Add `lp-cli fwcheck` now.
- Use subcommands/args like `lp-cli fwcheck list` and `lp-cli fwcheck run esp32c6 shader-compile-stress`.
- Optionally support shorthand later if it proves annoying.

### 4. Where should output go?

Context:
- user wants `traces` or similar
- existing profiling uses `profiles/`

Suggested answer:
- Write hardware-check runs under repo-root `traces/`.
- For each run, create a directory rather than one flat file:
  `traces/<timestamp>--<target>--<check>--<note>/`
- Always write `trace.txt`.
- Write `report.txt` when the check has a reporter or parseable records.
- Optionally write `records.jsonl` if structured records are extracted from mixed serial output.

### 5. Should firmware emit pure JSONL or mixed logs with embedded records?

Context:
- serial output includes bootloader logs, log prefixes, and ordinary human messages
- pure JSONL on the whole stream is unrealistic

Suggested answer:
- Use embedded check records with a recognizable prefix, for example:
  `[fw-check-json] { ... }`
- `lp-cli fwcheck` captures the full trace and extracts these records into `records.jsonl`.
- `fw-checks` with `std` can parse and summarize the extracted records.

## Notes from the user

- Prefer `lp-fw/fw-checks` and `FwCheck` naming.
- Use existing repo patterns with `check/` and `checks/name/...` where practical.
- Put actual shared code in `fw-checks` where possible.
- Use feature flags to keep firmware builds lean.
- Do not over-perfect the framework.
- Avoid Python; use Rust/Cargo-world tools.
- Support notes in trace output paths.
- Generate `report.txt` in cases where useful.
- For perf checks, JSONL-style output from firmware plus Rust-side summary is likely easiest.

## Additional design requirement

Adding a new check should be easy and documented:

1. Add a new directory/module under `lp-fw/fw-checks/src/checks/<name>/`.
2. Add the `FwCheck` enum variant and static config in `fw-checks`.
3. Add a feature flag to `fw-esp32` for that check.
4. Add a small ESP32 harness that initializes board/serial as needed and delegates to shared check code when possible.
5. Add or update the `lp-cli fwcheck` registry path only if the check needs target-specific behavior beyond the shared config.

The crate README and Rust module docs should explain:
- what firmware checks are for
- how they differ from unit tests/filetests/profiles
- how to add a new check
- how to emit structured records for reports
- how `lp-cli fwcheck` uses `FwCheckConfig`
