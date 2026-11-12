# LPVM Native Incremental Compile Notes

## Scope of work

Plan a resumable, budgeted `lpvm-native` compilation path that can be driven from engine ticks for background shader compilation on firmware targets.

The plan should cover:

- where to introduce resumable backend compile state in `lpvm-native`
- how the shader runtime should drive frontend + backend work under a per-tick budget
- how compiled artifacts are finalized and swapped in safely
- how to validate correctness, memory use, and tick-budget behavior

Out of scope for this plan:

- implementing the full feature itself
- broad compile optimizations unrelated to resumability
- changing shader language semantics
- reworking unrelated server/project sync behavior

## Current state of the codebase

### Frontend

- `lps-glsl` already has a resumable API:
  - [`lp-shader/lps-glsl/src/job.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lps-glsl/src/job.rs)
  - stages: `Lex -> Index -> Body -> Lower -> Done`
  - caller supplies a `CompileBudget`
- one-shot `lps_glsl::compile(...)` simply runs the job to completion:
  - [`lp-shader/lps-glsl/src/compile.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lps-glsl/src/compile.rs)

### Backend

- `lpvm-native` is still synchronous and module-at-once:
  - [`lp-shader/lpvm-native/src/compile.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm-native/src/compile.rs)
  - `compile_module(...)` builds `ModuleAbi`, then loops over every function and calls `compile_function(...)`
- `compile_function(...)` currently performs all major backend stages in one call:
  - LPIR const fold
  - lower to VInst
  - immediate folding
  - register allocation
  - emission
  - debug section construction
- register allocation and emission are also one-shot:
  - alloc: [`lp-shader/lpvm-native/src/regalloc/mod.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm-native/src/regalloc/mod.rs)
  - emit wrapper: [`lp-shader/lpvm-native/src/emit.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm-native/src/emit.rs)
- JIT finalization is also one-shot:
  - [`lp-shader/lpvm-native/src/rt_jit/compiler.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lpvm-native/src/rt_jit/compiler.rs)
  - flow today: `compile_module` -> `link_jit` -> `JitBuffer::from_code(...)`

### Runtime integration

- shader compilation in the engine is synchronous inside `ShaderNode::ensure_compiled(...)`:
  - [`lp-core/lpc-engine/src/nodes/shader/shader_node.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-core/lpc-engine/src/nodes/shader/shader_node.rs)
- when `shader` is `None`, render paths force a compile before use
- `ShaderNode::tick(...)` currently only updates config/state; it does not advance any compile job
- the graphics abstraction is synchronous:
  - trait: [`lp-core/lpc-engine/src/gfx/lp_gfx.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-core/lpc-engine/src/gfx/lp_gfx.rs)
  - native implementation: [`lp-core/lpc-engine/src/gfx/native_jit.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-core/lpc-engine/src/gfx/native_jit.rs)
- `LpsEngine` is also synchronous end-to-end:
  - [`lp-shader/lp-shader/src/engine.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-shader/lp-shader/src/engine.rs)

### Validation and profiling

- current compile perf logging exists around shader compile and native link:
  - `EVENT_SHADER_COMPILE` in shader node
  - `EVENT_SHADER_LINK` in native JIT compiler
- the old allocation profiling test is currently a placeholder:
  - [`lp-fw/fw-tests/tests/profile_alloc_emu.rs`](/Users/yona/dev/photomancer/feature/lightplayer-glsl-frontend/lp-fw/fw-tests/tests/profile_alloc_emu.rs)
- there is not yet a dedicated test harness for “compile progresses across ticks under a budget”

## Open questions

## Shader behavior while recompiling

- **Context:** the user clarified that this work is primarily for a future playlist/timeline feature where a next visual must be warmed up before activation. This is not mainly about improving ordinary edit-time compile-on-change behavior.
- **Suggested answer:** design the compiler/runtime seam so a caller can start and step a compile job ahead of activation. Normal edit-time compile UX can remain synchronous or visibly blocking for now; the important requirement is that the underlying compiler supports incremental progress when a future warm-up caller needs it.

## Backend resumability granularity

- **Context:** module-level resumability alone is probably not enough if one large function can still monopolize a tick. `compile_function(...)` is currently monolithic.
- **Suggested answer:** plan for resumability at least at backend stage boundaries per function (`ConstFold -> Lower -> Peephole -> Regalloc -> Emit -> Debug`), and design the state shape so finer intra-stage chunking can be added later without API churn.

## API seam for background compilation

- **Context:** `LpGraphics` and `LpvmEngine` are synchronous today. We need a place for a long-lived compile job that can be stepped each tick.
- **Suggested answer:** add explicit resumable compile job types rather than overloading the existing synchronous `compile_shader(...)` APIs. Keep one-shot APIs as convenience wrappers on top of job stepping.

## Validation target

- **Context:** user wants step time under `10ms`, ideally under `5ms`, on firmware. There is not yet a canonical test that proves this.
- **Suggested answer:** plan both deterministic unit/integration validation for “job yields and resumes correctly” and a firmware/emu profiling harness that records per-tick compile slices on representative shaders.

## Notes from the user that should influence the plan

- The motivation is preparation for background compilation, not just academic backend cleanup.
- The primary use case is future playlist/timeline visual switching, where the next visual needs to be warmed up before the current one ends.
- The exact warm-up API between playlist logic and shader nodes does not need to be designed in this plan; the important thing is enabling incremental compilation underneath.
- For ordinary compile-on-change, a visible pause is acceptable and may even be useful as feedback.
- A valuable deliverable is a new on-hardware firmware test that compiles representative example shaders incrementally and records per-tick compile time and memory usage.
- The user wants visibility into memory behavior as well as tick-time behavior during incremental compilation.
- Tick budget matters:
  - real concern if compile work blocks engine/server loop
  - target is `<10ms` per tick
  - preferred target is `<5ms` per tick
- The user explicitly called out `lpvm-native` as likely still non-incremental and wants that validated, not assumed.
- Validation is part of the request, not an afterthought.
