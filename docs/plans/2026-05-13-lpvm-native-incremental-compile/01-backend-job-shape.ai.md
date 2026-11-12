# Phase 1: Backend Job Shape

## Scope of phase

In scope:

- add a resumable `lpvm-native` compile job/state machine
- preserve the existing one-shot `compile_module(...)` path as a wrapper
- split backend compile orchestration into explicit staged boundaries
- keep final outputs equivalent to the current synchronous backend

Out of scope:

- shader-node or playlist warm-up integration
- frontend/backend combined orchestration in `lp-shader`
- on-hardware test harness
- deep micro-optimization inside lowering/regalloc unless needed for correctness

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep related compile-job types grouped under a dedicated `compile/` subtree if that improves readability.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

- Relevant existing files:
  - [lp-shader/lpvm-native/src/compile.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm-native/src/compile.rs)
  - [lp-shader/lpvm-native/src/emit.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm-native/src/emit.rs)
  - [lp-shader/lpvm-native/src/regalloc/mod.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm-native/src/regalloc/mod.rs)
  - [lp-shader/lpvm-native/src/rt_jit/compiler.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm-native/src/rt_jit/compiler.rs)
- Introduce resumable backend types such as:
  - `NativeCompileBudget`
  - `NativeCompileStage`
  - `NativeCompileStepResult`
  - `NativeCompileJob`
- The first implementation should resume at function-stage boundaries:
  - setup module ABI/sig map
  - const fold one function
  - lower one function
  - peephole one function
  - regalloc one function
  - emit one function
  - build debug sections for one function
  - finalize link/module
- Preserve completed intermediate outputs in job state so resumed compilation does not redo finished stages.
- Keep one-shot `compile_module(...)` behavior by driving `NativeCompileJob` to completion internally.
- Keep the public result shape compatible with current downstream users (`CompiledModule`, `CompiledFunction`, etc.).
- Add focused tests proving:
  - staged compile reaches the same output as one-shot compile for small modules
  - repeated `step(...)` calls make monotonic progress
  - job finalization produces a valid compiled module

## Validate

Run:

```bash
cargo fmt --all
cargo test -p lpvm-native
cargo check -p lpvm-native
```
