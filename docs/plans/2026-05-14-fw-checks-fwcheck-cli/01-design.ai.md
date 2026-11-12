# Firmware Check Design

## Shape

`lp-fw/fw-checks` is the shared vocabulary crate for firmware checks. It is
`no_std` by default so firmware can depend on it without pulling host support
into embedded builds, and it has a `std` feature for host-side report parsing.

The core types are:

- `FwCheck`: the stable name for a runnable check
- `FwCheckTarget`: the target family, starting with `esp32c6`
- `FwCheckConfig`: static metadata for features, done markers, trace names,
  supported targets, and record support

Check-specific shared code lives under `src/checks/<name>/`. Target-specific
board setup, heap sampling, cycle counters, and main-loop behavior stay in the
firmware crate unless they become obviously reusable.

## CLI

`lp-cli fwcheck` is the host runner.

```bash
lp-cli fwcheck list
lp-cli fwcheck run esp32c6 shader-compile-stress --note baseline
```

For ESP32-C6, the command resolves the serial port, builds the firmware with
the check's configured Cargo features, flashes it, opens the serial port itself,
normalizes newlines, captures the log, stops at the done marker, extracts
structured records, and writes a report.

Outputs are rooted at:

```text
traces/<timestamp>--<target>--<check>--<note>/
```

Each run writes:

- `trace.txt`: complete serial output
- `records.jsonl`: extracted `[fw-check-json]` records
- `report.txt`: a concise parsed summary when available

## Adding A Check

1. Add shared code under `lp-fw/fw-checks/src/checks/<name>/` if the check has
   reusable logic or report records.
2. Add a `FwCheck` variant and a `FwCheckConfig` entry.
3. Add a feature flag to the firmware crate, such as `fw-esp32`.
4. Add a small target harness that initializes the board/logging and delegates
   to shared code where possible.
5. Add a done marker if the check should run through `lp-cli fwcheck run`.
6. Emit `[fw-check-json]` records when a host summary is useful, and add a
   `std` reporter in `fw-checks` if the raw records need formatting.

## Current Scope

The first implemented check is `shader-compile-stress`. It exercises the
incremental shader compiler on ESP32-C6, captures per-case memory/latency
records, and generates a one-page text report. The `fw-emu` target is represented
in shared metadata where sensible, but its runner is intentionally deferred.
