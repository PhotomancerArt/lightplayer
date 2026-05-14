# LPVM Native Incremental Compile Design

## Scope of work

Design a resumable `lpvm-native` backend compile pipeline that can be stepped incrementally under a caller-provided budget, so future playlist/timeline warm-up code can compile upcoming visuals ahead of activation without blowing the engine tick budget.

This design covers:

- backend job/state shape inside `lpvm-native`
- shader-level orchestration across `lps-glsl` frontend and `lpvm-native` backend
- one-shot wrapper compatibility
- profiling and validation, including an on-hardware firmware test

This design does not define:

- the final playlist/timeline warm-up API
- UI/UX for ordinary edit-time compile pauses
- large independent backend optimizations unrelated to resumability

## File structure

```text
lp-shader/
  lpvm/
    src/
      compile_job.rs
      engine.rs
  lpvm-native/
    src/
      compile.rs
      compile/
        mod.rs
        module_job.rs
        function_job.rs
        stages.rs
      emit.rs
      regalloc/
        mod.rs
        incremental.rs
  lp-shader/
    src/
      engine.rs
      compile_job.rs
lp-core/
  lpc-engine/
    src/
      gfx/lp_gfx.rs
      gfx/native_jit.rs
      nodes/shader/shader_node.rs
lp-fw/
  fw-esp32/
    src/tests/
      test_incremental_shader_compile.rs
docs/
  plans/2026-05-13-lpvm-native-incremental-compile/
    00-notes.md
    00-design.md
    01-backend-job-shape.md
    02-shader-orchestration.md
    03-hardware-validation.md
    04-cleanup-and-final-validation.md
```

Notes:

- `compile.rs` in `lpvm-native` remains the one-shot facade.
- New `compile/` files hold resumable state machines and stage outputs.
- `regalloc/incremental.rs` is a deliberate seam even if the first implementation only resumes at stage boundaries.

## Architecture summary

The system becomes a two-layer resumable compiler:

1. `lps-glsl::CompileJob` handles GLSL -> LPIR incrementally.
2. `lpvm-native::NativeCompileJob` handles LPIR -> native module incrementally.

`lp-shader` owns a higher-level shader compile job that composes both layers:

- step frontend until LPIR is ready
- synthesise render helpers and validate shader/module shape
- step backend job until native module is ready
- publish final runnable shader/module

One-shot compile APIs remain in place by driving the resumable jobs to completion internally.

The key outcome is that future callers can start compile work early and step it over many ticks. That future caller may be a playlist/timeline warm-up system, but this plan intentionally keeps the compile mechanism independent of that product feature.

## Main components and interactions

### 1. Backend compile job

Add an explicit resumable backend job in `lpvm-native`:

- `NativeCompileJob`
- `NativeCompileBudget`
- `NativeCompileStage`
- `NativeCompileStepResult`

The first-cut job progresses at function-stage granularity:

- `SetupModule`
- `CompileFunctionConstFold`
- `CompileFunctionLower`
- `CompileFunctionPeephole`
- `CompileFunctionRegalloc`
- `CompileFunctionEmit`
- `CompileFunctionDebug`
- `LinkModule`
- `Done`

Why this granularity:

- much smaller work units than the current monolithic `compile_function(...)`
- simple explicit state
- enough control to make tick budgets meaningful
- preserves room for deeper incrementalization later

### 2. Per-function staged state

Introduce a per-function job state carrying intermediate artifacts:

- original `IrFunction`
- optionally const-folded function
- lowered function
- post-peephole lowered function
- allocation result
- emitted machine code
- debug sections

This avoids recomputing completed work when a compile is resumed.

### 3. Final link/finalize stage

Treat final linking/JIT image creation as its own resumable stage rather than hiding it inside “done”.

That stage covers:

- collecting compiled function outputs
- resolving relocations against builtins and intra-module entries
- constructing the final `JitBuffer`
- building module debug info

Even if link remains one-shot initially, it should be represented as a distinct stage so it can be profiled and, if needed, sliced later.

### 4. LPVM engine and shader-level orchestration

Add a resumable compile contract above raw backend internals.

At the `lpvm` / `lp-shader` layer:

- add backend-agnostic compile-job types or traits
- expose a shader-level compile job that owns:
  - source/options
  - `lps-glsl::CompileJob`
  - post-lowering synth/validation state
  - `lpvm-native::NativeCompileJob`

This keeps future warm-up callers out of backend details.

### 5. Shader node integration

Do not require the full warm-up feature in this plan.

Instead, shape the API so `ShaderNode` or a future visual/playlist manager can:

- create a compile job
- step it with a budget during ticks
- query status/progress/error
- finalize and swap in the compiled shader when ready

Ordinary edit-time compile-on-change can remain synchronous for now. The important design constraint is that synchronous compile becomes a wrapper over the same staged pipeline, not a separate implementation.

### 6. Validation and profiling

Validation has three layers:

#### Unit and backend equivalence tests

- staged compile job reaches the same final result as one-shot compile
- pause/resume works at every stage boundary
- function compilation preserves outputs across resumed execution
- final linked module matches one-shot symbol/entry expectations

#### Emulator / host integration tests

- shader-level compile job can advance over repeated steps and eventually render successfully
- partial progress does not expose half-built modules

#### On-hardware firmware test

Add a dedicated `fw-esp32` test that:

- compiles representative example shaders incrementally
- uses a fixed per-tick compile budget
- logs:
  - tick time spent advancing compile
  - compile stage / function index
  - free / used memory per tick
  - total ticks to completion
- exercises several shader sizes, ideally:
  - tiny/trivial
  - `examples/basic`
  - one texture-using example if practical

This is a primary deliverable of the work, because it validates the actual firmware behavior that motivated the design.

## Design constraints

- Keep the compiler `no_std` compatible across the runtime path.
- Do not fork logic into separate synchronous and incremental implementations; the one-shot path should run the resumable jobs to completion.
- Preserve current compilation correctness before chasing smaller time slices inside difficult stages like regalloc.
- Keep stage outputs explicit and inspectable for profiling/debugging.
- Make the first version good enough for warm-up orchestration, then refine stage internals only if measured tick slices still exceed target.

## Expected evolution

The likely first implementation will be incremental at stage boundaries, not instruction-by-instruction within lowering/regalloc/emission.

If profiling shows a single backend stage still exceeds the desired `<5ms` tick target on representative shaders, the next refinement points are:

- chunk lower over LPIR op ranges
- chunk regalloc over region-tree traversal
- chunk final link over functions/relocations

This design keeps those upgrades possible without reworking the public job model.
