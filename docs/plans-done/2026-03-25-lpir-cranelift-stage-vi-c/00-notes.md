# Plan notes: lpir-cranelift Stage VI-C (ESP32 hardware validation)

Roadmap: [stage-vi-c-esp32.md](../../roadmaps-old/2026-03-24-lpir-cranelift/stage-vi-c-esp32.md)

## Scope of work

- Point `fw-esp32` at the new compiler story: remove dead `lp-glsl-cranelift` /
  direct Cranelift optional deps (or replace only if something in-tree still
  needs them — currently none appear wired to features).
- Build and flash ESP32-C6 firmware with `lp-server` + `lp-engine` (already
  `lpir-cranelift`, `default-features = false` on the embedded path).
- On-device validation: shaders compile, render, no OOM; optional light heap
  checks only (not the primary memory A/B).
- A/B vs old compiler: document in `docs/reports/`; **primary** memory profile and
  compile-time deltas on **fw-emu** (relative differences treated as indicative
  for ESP32). ESP32: firmware binary size, correctness, integration/triage.

## Current state (codebase)

- **`lp-engine`:** Depends on `lpir-cranelift` with feature forwarding for
  optimizer/verifier/std; old `lp-glsl-cranelift` / `cranelift-codegen` /
  `lp-glsl-jit-util` removed from this crate.
- **`lp-server`:** `default-features = false` on `fw-esp32` with
  `features = ["panic-recovery"]` only — optimizer/verifier off for size (matches
  roadmap intent).
- **`fw-esp32/Cargo.toml`:** Still declares optional `lp-glsl-cranelift`,
  `lp-glsl-jit-util`, `lp-glsl-builtins`, `cranelift-codegen`, `cranelift-frontend`,
  `cranelift-module`, `cranelift-control`, `target-lexicon`. No `[features]`
  entry enables these dependencies, so they are **orphan optional deps** (likely
  pre–VI-B leftovers). Shader compilation path is **transitive**:
  `fw-esp32` → `lp-server` → `lp-engine` → `lpir-cranelift`.
- **Rust sources under `fw-esp32/src`:** No imports of `lp_glsl_cranelift`; JIT
  host helpers (`jit_fns.rs`, log bridges) remain relevant for generated code.

## Questions (to resolve in order)

### Q1 — `fw-esp32` manifest cleanup

**Context:** Optional compiler crates are unused by any feature; the active path
is `lp-engine` → `lpir-cranelift`.

**Suggested answer:** Delete the orphan optional dependency block from
`fw-esp32/Cargo.toml` (and `lp-glsl-builtins` if nothing enables it). Do **not**
add a duplicate `lpir-cranelift` edge unless a future `fw-esp32` binary needs
to call the compiler API directly (it should not for VI-C).

**Answer:** Yes — delete all orphan deps now; keep migrating toward fully
dropping the old compiler stack from the tree once the new path is validated on
hardware (VI-C).

### Q2 — Location and format of the A/B comparison document

**Context:** Roadmap asks for binary size, memory, compile time, execution
speed, plus known issues.

**Suggested answer:** Add `docs/reports/2026-03-25-lpir-cranelift-vi-c-ab.md` (or
date the file when measurements are taken) with a small table per metric,
measurement method, old vs new, and a “Known issues / follow-ups” section. Link it
from `summary.md` when the plan completes.

**Answer:** Yes — use `docs/reports/<YYYY-MM-DD>-lpir-cranelift-vi-c-ab.md` (date
when measured), tables + methodology + known issues, link from plan `summary.md`.
The **most important A/B signal is memory profiling**, captured from **fw-emu**
(not as the primary metric on ESP32). Compile time and similar deltas are also
fine to measure on **fw-emu**; **relative** old-vs-new differences are expected
to be representative of ESP32 even if absolutes differ.

### Q3 — Where we measure compilation memory / timing

**Context:** Original roadmap text mentioned peak heap on device. User direction:
detailed memory profile belongs on **fw-emu**; timing/compile comparisons there
are sufficient for A/B. ESP32 still needs correctness and “no OOM” confidence.

**Suggested answer:** Document in the A/B report: **fw-emu** = primary memory
profile + compile-time (and execution) comparison between compilers/worktrees.
**ESP32** = flash/build, visual correctness, and OOM smoke; optional light heap
snapshot only if cheap, not a gate for the memory story.

**Answer:** Align with that split: quantitative A/B on fw-emu; hardware for
correctness and integration risks.

### Q4 — Gate: `fw-emu` validated before hardware work

**Context:** Roadmap lists VI-B (including fw-emu) as a dependency.

**Suggested answer:** Treat VI-C implementation as starting only after
`cargo build` / smoke for `fw-emu` on the RISC-V target passes on the branch;
if not, first fix VI-B regressions.

**Answer:** Yes — **fw-emu** build + smoke is a gate before relying on hardware.
Hardware tests are **manual** (user); the plan should front-load every automated
check possible so device time is minimal and high-signal.

## Notes

- Q1: Remove orphan `fw-esp32` compiler deps immediately; full removal of old
  crates from the repo waits on hardware validation.
- Q2/Q3: Memory and compile-time A/B primarily on **fw-emu**; ESP32 for
  correctness, firmware size, and real-world integration (not primary memory
  profiling).
- Q4: Gate hardware on fw-emu green; maximize automated validation first;
  on-device work is manual.
