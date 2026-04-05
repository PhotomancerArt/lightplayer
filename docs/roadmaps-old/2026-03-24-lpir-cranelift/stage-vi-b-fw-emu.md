# Stage VI-B: fw-emu Runs New Compiler

## Goal

Migrate `lp-engine` from `lps-cranelift` to `lpir-cranelift` and validate
by running `fw-emu` — the firmware compiled to RV32 and executed in
`lp-riscv-emu` on desktop. This proves the full embedded compilation path
(GLSL → LPIR → CLIF → RV32 JIT on device) works end-to-end **without
requiring ESP32 hardware**.

## Suggested plan name

`lpir-cranelift-stage-vi-b`

## Scope

**In scope:**

- **lp-engine migration:**
  - Replace `lps-cranelift` dependency with `lpir-cranelift`
  - Update `ShaderRuntime` to use `jit()` / `jit_from_ir()` → `JitModule`
  - Update render loop to use `DirectCall` (Level 3) for fast path
  - Wire `Q32Options` from `GlslOpts` → `CompileOptions`
  - Update `GlslValue` marshalling for the new API
  - Remove `cranelift-codegen` direct dependency from `lp-engine` if
    `DirectCall` successfully abstracts calling convention
- **lp-server Cargo.toml:**
  - Forward new features (`std`, `cranelift-optimizer`, `cranelift-verifier`)
    from `lpir-cranelift` instead of `lps-cranelift`
- **fw-emu validation:**
  - Update `fw-emu` deps if needed (it depends on `lp-server` → `lp-engine`,
    so the compiler swap is mostly transitive)
  - Build and run `fw-emu` — shaders compile and render correctly
  - Run existing `fw-emu` integration tests
- **Desktop host JIT:** Verify `lp-engine` still works in desktop/`std` mode
  (e.g. existing `lp-engine` tests, `just test`)

**Out of scope:**

- ESP32 hardware testing (Stage VI-C)
- A/B performance comparison (Stage VI-C)
- `no_std` / embedded readiness of `lpir-cranelift` itself (Stage VI-A —
  prerequisite)
- Native f32 mode, Q32 wrapping mode
- Old compiler deletion (Stage VII)

## Key decisions

- lp-engine migration is a clean swap: replace dependency, update call sites,
  no feature flags for A/B switching. Git worktree comparison can be done in
  VI-C.
- `fw-emu` is the primary validation target because it exercises the full
  embedded path (`no_std`, RV32 ISA, `AllocJitMemoryProvider`, memory-
  constrained compilation) but runs on desktop — fast iteration, no hardware
  required.

## Deliverables

- `lp-engine` compiles against `lpir-cranelift` instead of `lps-cranelift`
- `fw-emu` builds, runs, and renders shaders correctly
- Desktop host JIT (`lp-engine` tests) still passes
- Known issues list (regressions, if any)

## Dependencies

- Stage VI-A (lpir-cranelift embedded readiness) — `std` feature gating,
  ISA selection, `CompileOptions` expansion, memory strategy
- Stage V2 (filetests on `jit.q32` and `rv32.q32`) — correctness baseline

## Estimated scope

~500 lines of engine migration + ~100 lines of dependency plumbing. Plus
debugging.
