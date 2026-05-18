# Phase 2: Shader Orchestration

## Scope of phase

In scope:

- compose resumable `lps-glsl` and `lpvm-native` compile jobs into a shader-level stepped compile flow
- add API seams in `lpvm` / `lp-shader` for resumable compilation
- keep existing one-shot compile wrappers working

Out of scope:

- final playlist/timeline warm-up API
- broad shader-node UX changes for ordinary compile-on-change
- on-hardware measurement harness

## Code organization reminders

- Prefer granular files with one main concept per file.
- Keep orchestration code separate from backend implementation details.
- Put helpers lower in the file when that improves readability.
- Mark any temporary code with a clear `TODO`.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

- Relevant files:
  - [lp-shader/lpvm/src/engine.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm/src/engine.rs)
  - [lp-shader/lp-shader/src/engine.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lp-shader/src/engine.rs)
  - [lp-core/lpc-engine/src/gfx/lp_gfx.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-core/lpc-engine/src/gfx/lp_gfx.rs)
  - [lp-core/lpc-engine/src/gfx/native_jit.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-core/lpc-engine/src/gfx/native_jit.rs)
  - [lp-shader/lps-glsl/src/job.rs](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lps-glsl/src/job.rs)
- Add a shader-level compile job that owns:
  - source/options
  - `lps-glsl::CompileJob`
  - post-lowering synth/validation state
  - `lpvm-native` backend job
- One-shot `compile_px_desc(...)` should become a wrapper over the new stepped flow.
- Keep the API independent of playlist logic; callers should be able to create/step/query/finalize a compile job without knowing backend details.
- Prefer explicit job/result/status types over overloading the existing synchronous trait method semantics.
- Add tests covering:
  - stepped shader compile reaches the same final runnable shader/module as one-shot compile
  - frontend completion cleanly hands off to backend compilation
  - failure propagation preserves good diagnostics

## Validate

Run:

```bash
cargo fmt --all
cargo test -p lp-shader
cargo check -p lp-shader
cargo check -p lpc-engine
```
