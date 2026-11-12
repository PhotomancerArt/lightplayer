# Milestone 5 - Embedded Scheduling and Measurement

## Title and Goal

Run `lps-glsl` as a cooperative compile job on emulator and ESP32, with timing and memory
evidence against the current Naga path.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-glsl-frontend/m5-embedded-scheduling-and-measurement/`

## Scope

In scope:

- Make every frontend stage compatible with `no_std + alloc`.
- Add allocation and timing instrumentation around `lps-glsl` stages.
- Exercise `lps_glsl::CompileJob::step(...)` with a configurable budget.
- Integrate a non-production scheduler path that can compile an example shader across render frames.
- Measure compile latency, peak memory pressure, and longest step duration on emulator and ESP32.
- Compare against the current Naga frontend for the same examples where possible.

Out of scope:

- Making `lps-glsl` the default production path.
- Playlist policy and UX beyond a minimal compile-cache experiment.
- Fine-grained expression-level yielding unless measurements require it.

## Key Decisions

- Synchronous compile stays as a wrapper over the job API.
- The first scheduling target is coarse step yielding; finer granularity is measurement-driven.
- Binary size pressure must be solved by dependency and feature audit, not by disabling the compiler.

## Deliverables

- Emulator and ESP32 measurement report.
- Compile-job budget tests.
- Evidence of rendering continuing while a future shader compiles.
- Decision notes on whether step granularity is sufficient.

## Dependencies

- Milestone 4 example compatibility.
- Existing firmware/emulator compile and render harnesses.

## Execution Strategy

Full plan. This touches runtime scheduling, firmware behavior, and measurement, so it should be
planned as a coordinated integration milestone.
