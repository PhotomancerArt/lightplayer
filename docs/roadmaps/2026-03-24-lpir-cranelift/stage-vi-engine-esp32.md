# Stage VI: lp-engine Migration and ESP32 Validation

## Goal

Migrate lp-engine from the old `lp-glsl-cranelift` to the new
`lpir-cranelift` crate. Test on ESP32 hardware. Run A/B performance
comparisons against the old compiler. (`rv32.q32` filetests are Stage V2.)

## Suggested plan name

`lpir-cranelift-stage-vi`

## Scope

**In scope:**
- **lp-engine migration**:
  - Replace `lp-glsl-cranelift` dependency with `lpir-cranelift`
  - Update `ShaderRuntime` to use `jit()` → `JitModule`
  - Update render loop to use `DirectCall` (Level 3) for fast path
  - Update `GlslValue` marshalling to use `GlslQ32` if needed, or
    use the direct call path exclusively
  - Remove `cranelift_codegen` direct dependency from lp-engine if
    `DirectCall` successfully abstracts calling convention
  - Verify desktop host JIT still works
- **ESP32 firmware**:
  - `lp-server` → `lp-engine` picks up new crate transitively
  - Build `fw-esp32` with new compiler
  - Run on device, verify shaders render correctly
  - Monitor compilation memory usage on device
- **A/B performance comparison**:
  - Git worktree on main with old compiler
  - Compare: binary size (firmware image), compilation memory peak,
    compilation time, shader execution speed (frame time)
  - Document results

**Out of scope:**
- Optimization of the LPIR path (if slower, document and defer)
- Native f32 mode
- Q32 wrapping mode
- Old compiler deletion (Stage VII)

## Key decisions

- lp-engine migration is a clean swap: replace dependency, update call
  sites, no feature flags.
- The `DirectCall` interface should be sufficient for lp-engine's render
  loop. If it's not (e.g., the overhead of the trampoline is measurable),
  we can expose the raw function pointer + metadata as a fallback.
- Object emission reuses Cranelift's `ObjectModule`. The emitter
  (`emit.rs`) is target-agnostic — same CLIF, different module type.
- Builtins linking for RV32 follows the same pattern as the old crate:
  embedded builtins ELF + shader object → merged symbol map.

## Open questions

- **`no_std` readiness**: lp-engine on ESP32 runs without `std`. The new
  crate's `jit()` function depends on naga (which needs `std`? — needs
  verification). The old compiler ran naga on ESP32 somehow. Verify the
  dependency chain works on `no_std` + ESP-IDF.
- **Streaming compilation**: The old compiler had `glsl_jit_streaming`
  which compiled functions one at a time to reduce peak memory. The new
  crate's per-function lowering (biggest first) achieves similar goals
  but differently. Is there an equivalent concern for the Cranelift
  compilation step (cranelift-codegen's internal memory usage per
  function)? The old streaming approach compiled and finalized functions
  individually — our batch-define-then-finalize may use more memory.
  Investigate.
- **Object module builtins linking**: The old crate had
  `builtins_linker.rs` using `lp-riscv-elf` to merge ELF objects. Do
  we copy this logic or find a cleaner approach? The linking is
  mechanical but fiddly. Consider extracting to a shared utility.
- **ESP32 arch flags**: `ShaderRuntime` currently uses `target_override:
  None`, which defaults to `riscv32imafc` via `riscv32_triple()`. ESP32
  might need `riscv32imac` (no FPU). The old compiler had
  `GlslOptions::host_jit_embedded_riscv32()` for this but it wasn't
  wired in. Worth getting right this time.
- **Compilation time**: The old compiler did AST→CLIF in one pass. The
  new path does GLSL→Naga→LPIR→CLIF (three passes over the source).
  This may be slower to compile. Measure.
- **Binary size**: Additional crate dependencies (naga, lpir) increase
  the firmware binary. Measure the delta. If naga was already linked
  (the old compiler used `lp-glsl-frontend` + `glsl` crate, not naga),
  the delta may be smaller than expected — or larger if naga is bigger
  than the old parser.

## Deliverables

- lp-engine using the new compiler crate
- fw-esp32 building and running shaders
- A/B comparison document: binary size, memory, compilation time,
  execution speed
- Known issues list (regressions, if any)

## Dependencies

- Stage V2 (filetests on `jit.q32` and `rv32.q32`) — host and emulator
  correctness validated
- Stage V1 (object + emulator in `lpir-cranelift`) — already required by V2

## Estimated scope

~500 lines of engine migration + ~200 lines of test/benchmark
infrastructure. Plus debugging.
