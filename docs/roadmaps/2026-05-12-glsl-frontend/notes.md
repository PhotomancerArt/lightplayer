# LightPlayer GLSL Frontend Experiment - Notes

## Scope

Explore a custom, performance-oriented LightPlayer shader frontend for embedded ESP32-C6 runtime
compilation.

The goal is not to replace the product's on-device compiler with host precompilation. The goal is
to preserve the core GLSL-on-device JIT product while reducing compile latency and making compile
work schedulable across frames.

This roadmap starts as an experiment. It should answer:

- How much of the current compile cost is avoidable by replacing the Naga GLSL frontend?
- Can a custom frontend compile the existing LightPlayer examples quickly enough to hot-load before
  transitions?
- Can compilation be structured as small resumable units so the single ESP32-C6 core does not wreck
  frame timing?
- Can Naga remain the correctness oracle while embedded builds use a smaller, LightPlayer-shaped
  frontend?

The goal is not full compatibility with every existing filetest. Filetests remain the validity
corpus and should be sampled aggressively for the language features used by examples, but the
product target for this roadmap is compiling the current example shaders and a small amount of
nearby syntax that makes the implementation coherent.

## Current State

The current product path is:

```text
GLSL source
  -> lps-frontend source prep
  -> Naga glsl-in parse
  -> Naga module metadata extraction
  -> lps-frontend Naga -> LPIR lowering
  -> render texture / sample wrapper synthesis
  -> lpvm-native LPIR -> RV32 codegen
  -> JIT link and publish
```

Important current files:

- `lp-shader/lps-frontend/src/parse.rs` prepares GLSL for Naga, including LPFX prologue insertion
  and compatibility rewrites.
- `lp-shader/lps-frontend/src/lower.rs` lowers `NagaModule` into LPIR and `LpsModuleSig`.
- `lp-shader/lp-shader/src/engine.rs` owns the high-level `compile_px_desc` path.
- `lp-core/lpc-engine/src/nodes/shader/shader_node.rs` wraps shader compilation in the
  `shader-compile` perf event.
- `lp-shader/lpvm-native/src/compile.rs` and `lp-shader/lpvm-native/src/rt_jit/compiler.rs` own
  native compile and link.

Recent emulator profiles showed Naga/front-end costs dominating a large fraction of compile, but
real hardware is worse than the emulator estimate. `examples/basic/shader.glsl` is about 3922 bytes
and has been observed on real hardware at:

```text
[shader-node] compilation starting (node=NodeId(3), 3922 bytes)
[shader-node] compilation succeeded (node=NodeId(3), elapsed=611ms)
```

That latency is tolerable at startup but dangerous for future playlist hot-loading. A single-core
ESP32-C6 cannot hide a monolithic 600ms compile behind rendering.

There is an older LightPlayer fork of an OSS parser at:

```text
/Users/yona/dev/photomancer/glsl-parser
```

That fork is a no_std-capable `glsl` crate based on `nom`, with span support. It used to support
the legacy frontend before LPIR/Naga. It may be useful as a reference or a host-side parser, but
the embedded endpoint should probably not be a full AST-first GLSL parser again.

## Working Hypothesis

A custom frontend can help, but the main product requirement is not just lower total compile time.
The compiler must become schedulable.

The likely winning combination is:

- no source-injected builtin prelude
- reachability-first compilation from `render`
- compact token tape and arena allocation
- direct typed lowering to LPIR
- per-function / per-statement resumable compilation jobs
- Naga retained as a host correctness oracle

## Proposed Architecture

### Frontend Shape

Build a new LightPlayer-specific frontend rather than a general-purpose GLSL compiler.

Potential crate name:

```text
lp-shader/lps-glsl
```

Potential API shape:

```rust
pub fn compile_light_glsl(
    source: &str,
    options: &LightFrontendOptions,
) -> Result<(lpir::LpirModule, lps_shared::LpsModuleSig), LightFrontendError>;
```

Later, a resumable API can expose a job:

```rust
pub struct CompileJob { ... }

impl CompileJob {
    pub fn step(&mut self, budget: CompileBudget) -> CompileStepResult;
}
```

### Pipeline

```text
source
  -> lexer/token tape
  -> top-level index
       uniforms
       globals
       structs/types
       function signatures
       function body source spans
  -> reachability from render and synthetic entry needs
  -> typed parse/lower of reachable function bodies
  -> LPIR + LpsModuleSig
  -> existing lpvm-native backend
```

### Token Tape

Use a compact token tape instead of a full owned AST:

- token kind
- source span
- optional interned identifier id
- optional literal payload

Avoid repeated string copies. Intern identifiers once. Prefer arena allocation for short-lived
compiler data and free the whole arena after compile.

### Top-Level Index First

The first pass should avoid parsing function bodies deeply. It records declarations and body spans:

- uniform declarations and layout bindings
- global declarations / consts
- struct definitions
- function prototypes and definitions
- parameter qualifiers
- return types

Then build a call graph and lower only functions reachable from `render` plus required synthetic
wrappers. This matters because real shaders can include helpers or alternate demos that are not
active in the current render path.

### Builtins Without Source Injection

Do not prepend LPFX source text on embedded builds. Register known builtins in a compact table:

- GLSL math builtins (`sin`, `cos`, `mix`, `clamp`, etc.)
- LPFN functions (`lpfn_fbm`, `lpfn_psrdnoise`, etc.)
- texture functions
- VM/system functions if needed

The compiler should resolve these as intrinsic/import declarations directly into LPIR metadata.

This avoids repeatedly lexing and parsing a prologue the product already knows.

### Direct Typed Lowering

For function bodies, use a small statement parser plus a Pratt expression parser.

Type resolution and LPIR emission should happen together where possible:

- scalarize vectors and matrices as they are typed
- emit LPIR ranges directly
- resolve overloads using the LightPlayer builtin table
- avoid a general Naga-like IR when LPIR is the actual backend contract

For complex control flow, a tiny typed HIR may be useful, but only where direct emission becomes
awkward.

### Resumable Compilation

Compilation should be broken into small work items:

- lex next source chunk
- scan next top-level declaration
- parse one signature
- parse/lower one statement or expression subtree
- compile one LPIR function
- link and publish

The engine/playlist can then give the compiler a frame budget, for example 1-2ms after output
flush, and continue rendering between steps.

This is the key difference between "faster compiler" and "usable hot-load compiler" on one CPU.

### Correctness Strategy

Keep Naga as the correctness oracle on host and in filetests:

```text
GLSL -> Naga -> LPIR
GLSL -> Light frontend -> LPIR
```

Compare:

- LPIR shape where stable
- metadata (`LpsModuleSig`, uniforms, texture specs)
- rendered/sample outputs
- diagnostics for expected errors

The embedded fast path can start as a strict subset. Unsupported syntax should produce clear errors
and should not silently fall back to host-only behavior on device.

## Relationship to the Old `glsl-parser` Fork

The old fork is valuable context:

- already no_std-capable
- has span support
- parses broad GLSL450/460 syntax
- used by existing tooling such as `lps-builtins-gen-app`

But it is probably not the ideal embedded runtime endpoint because it is AST-first and fairly broad.

Possible uses:

- reference parser for syntax behavior
- source of grammar knowledge and tests
- host-only comparison tool
- temporary bootstrap for top-level parsing if the experiment needs a faster first milestone

Avoid committing too early to carrying a full AST on ESP32.

## Milestone Sketch

### M0 - Measurements and Seams

- Add finer perf events around current compile phases:
  - source prep
  - Naga parse
  - metadata extraction
  - Naga -> LPIR lowering
  - synth wrappers
  - native compile
  - native link
- Add real-hardware logging where possible.
- Identify exact RAM/allocator pressure during compile.

### M1 - Host Prototype for Real Shader Subset

- Create experimental frontend crate.
- Parse top-level declarations and function signatures.
- Build a function index and call graph.
- Compile a tiny subset to LPIR:
  - uniforms
  - scalar/vector locals
  - constructors
  - calls
  - returns
  - arithmetic
- Differential-test against Naga for a handful of filetests and `examples/basic`.

### M2 - Direct LPIR for Useful Playlist Shaders

- Add if/for/while support needed by examples.
- Add swizzles, vector component access, assignment, inc/dec as needed.
- Add builtin resolution for common GLSL and LPFN functions.
- Add static reachability filtering.
- Measure host and emulator compile time against Naga.

### M3 - Embedded Fast Path

- Make the crate `no_std + alloc`.
- Audit allocation patterns.
- Add arena/token tape implementation.
- Compile and run on `fw-emu` and ESP32-C6.
- Keep Naga path available where appropriate, but do not require it for the embedded fast subset.

### M4 - Resumable Compile Job

- Introduce `lps_glsl::CompileJob`.
- Add per-step budget and yield points.
- Integrate with engine/playlist lookahead.
- Publish compiled shader atomically once complete.
- Validate steady-render frame timing while compiling next shader in the background.

### M5 - Cleanup and Product Decision

- Decide whether the custom frontend remains an experiment, becomes the default embedded frontend,
  or is used only for fast-subset playlist shaders.
- Document unsupported GLSL.
- Expand filetest coverage.
- Keep Naga oracle tests in CI for the shared subset.

## Open Questions

### Q1 - GLSL subset or WGSL-like language?

Context: GLSL is the current user-facing language and all examples are GLSL. WGSL has a cleaner
grammar, but moving user-facing language has product and compatibility costs.

Suggested answer: Start with a GLSL subset. Keep the internal parser/compiler architecture generic
enough that a WGSL-like frontend could emit the same typed lowering later.

### Q2 - Use old `glsl-parser` fork or write a smaller parser?

Context: The fork is no_std and already has spans, but it is AST-first and broad.

Suggested answer: Use it as reference and possibly for host-side comparison, but build the embedded
frontend around a token tape, top-level index, and direct typed lowering.

### Q3 - How strict should the fast subset be?

Context: Silent semantic gaps are dangerous in a shader compiler. Naga's strength is correctness.

Suggested answer: Be strict. Unsupported syntax must produce a precise diagnostic. Host tests should
compare against Naga for everything in the supported subset.

### Q4 - What compile-time target is good enough?

Context: A friend has a custom C GLSL compiler on ESP32 in the ~10ms range, but with a smaller
feature set. LightPlayer does more and should not optimize only for cold total time.

Suggested answer: Track two targets:

- simple shaders: approach ~10-30ms cold compile
- real LightPlayer shaders like `examples/basic`: first goal below 100-150ms, then below 50ms if
  reachable-only and allocation wins pan out

For hot-load UX, also require compile steps to stay within a small frame budget.

### Q5 - Where should scheduling live?

Context: Shader compilation is currently synchronous in `ShaderNode::ensure_compiled`.

Suggested answer: Keep synchronous compile for startup and tests, but add a playlist/lookahead
compile cache that owns `lps_glsl::CompileJob` instances and publishes completed shaders before
transitions.

## User Notes

- Startup compile around 500-600ms can be acceptable.
- Hot-loading shaders during playlist transitions is the concerning case.
- ESP32-C6 has one CPU, so background compilation must mean cooperative scheduling, not true
  parallelism.
- Naga has been valuable for correctness, but may be the wrong embedded runtime dependency.
- Replacing Cranelift with `lpvm-native` showed that a targeted custom compiler can be feasible and
  worthwhile when the general-purpose tool is too heavy.
- The custom frontend should preserve the non-negotiable product requirement: GLSL shaders compile
  and execute on device at runtime.
- The roadmap goal is compiling existing examples, plus selected filetests where they prove needed
  validity. It is explicitly not full filetest compatibility.
- Resumability should be designed in from day one. The initial yield resolution can be coarse, but
  the architecture should not assume one monolithic compile call.
- Diagnostics should carry source spans and best-effort messages. Halt-on-first-error is acceptable
  initially; recovery and multi-error reporting can come later.
- A future WGSL frontend is nice to enable. Shared semantic/lowering infrastructure is valuable,
  but GLSL syntax should not be over-abstracted prematurely.

## Example Compatibility Surface

Current example shaders use:

- `layout(binding = N) uniform` declarations for scalar, bool, int, and vector uniforms.
- Global `const` declarations and function-local `const` declarations.
- Scalar types: `float`, `int`, `uint`, `bool`.
- Vector types: `vec2`, `vec3`, `vec4`.
- Function definitions, calls before definitions, early returns, and nested calls.
- `if` / `else`, including early-return `if` chains.
- `for` loops, including nested loops in `examples/rocaille/shader.glsl`.
- Local declarations with and without initializers.
- Assignment, compound assignment, and component assignment such as `color.a = 1.0`.
- Swizzles such as `.x`, `.y`, `.yx`, and vector component reads.
- Constructors and casts such as `vec3(...)`, `vec4(...)`, `float(i)`, and `int(a)`.
- Builtins used by examples: `abs`, `atan`, `clamp`, `cos`, `exp`, `floor`, `fract`, `length`,
  `max`, `min`, `mix`, `mod`, `sin`, `smoothstep`, and `tanh`.
- LPFN calls used by examples: `lpfn_fbm`, `lpfn_hsv2rgb`, `lpfn_psrdnoise`, `lpfn_worley`.
- Out/inout-style LPFN arguments, currently visible through the `lpfn_psrdnoise(..., gradient, ...)`
  call pattern.

Current examples do not require full structs, arrays, textures, preprocessor support, GPU-stage
metadata, matrices, switch statements, or full GLSL layout semantics.
