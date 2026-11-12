# Stage VI-A: lpvm-cranelift Embedded Readiness

## Goal

Make `lpvm-cranelift` compile and run correctly in an **embedded** profile
(`no_std` + `alloc`, explicit ISA, aggressive memory strategy) so that the
engine migration (VI-B, VI-C) is a clean dependency swap — not also a "make
the compiler work on constrained hardware" effort.

## Suggested plan name

`lpvm-cranelift-stage-vi-a`

## Scope

**In scope:**

- **`std` Cargo feature (default):** Gate `std`-dependent code behind a
  default `std` feature, following Rust ecosystem convention. Without `std`,
  the crate compiles with `no_std` + `alloc`. Cranelift sub-crates
  (`cranelift-codegen`, `cranelift-frontend`, `cranelift-jit`) already
  support `no_std` — wire the feature flags through.
- **ISA selection:** Replace hard-wired `cranelift-native` auto-detect with
  explicit ISA construction. With `std`, auto-detect remains available as a
  convenience helper; without `std`, callers pass target triple + flags
  directly (e.g. `riscv32imac` for ESP32-C6).
- **JIT memory provider:** Support `AllocJitMemoryProvider` (or equivalent)
  when `mmap` is unavailable. The old crate used this on `no_std`.
- **Memory-optimized define:** Extend the per-function lowering loop to strip
  CLIF metadata after define (match old crate's `memory_optimized` behaviour).
  Ensure `IrFunction` + CLIF are both dropped before the next function.
- **`CompileOptions` expansion:**
    - `Q32Options` (add_sub mode, mul mode, div mode) — equivalent to old
      `GlslOptions` Q32 fields. Needed so the engine can pass per-shader
      arithmetic preferences.
    - `max_errors: Option<usize>` — bound diagnostic count, critical for
      embedded where large error vectors can OOM.
    - `memory_strategy: MemoryStrategy` (enum: `Default`, `LowMemory`) —
      runtime knob so the same binary can choose based on environment.
- **Optimizer / verifier gating:** Expose `cranelift-optimizer` and
  `cranelift-verifier` as opt-in Cargo features (binary size savings when
  omitted — already done in `fw-esp32` for the old crate).
- **Streaming / per-function finalize:** Investigate whether Cranelift JIT
  supports per-function `finalize_definitions` or an equivalent that frees
  compiled code buffers earlier. If it does, use it in `LowMemory` mode.
  If not, document the gap and measure the peak memory delta vs the old
  compiler.

**Out of scope:**

- Engine migration (Stage VI-B / VI-C)
- A/B performance comparison (Stage VI-C)
- Native f32 mode, Q32 wrapping mode
- Old compiler deletion (Stage VII)

## Deliverables

- `lpvm-cranelift` compiles with `--no-default-features` (i.e. without
  `std`) targeting `riscv32imac`
- `rv32.q32` filetests pass with the embedded feature profile
- `CompileOptions` has `Q32Options`, `max_errors`, memory strategy
- Documentation of any streaming-finalize gaps vs old compiler

## Dependencies

- Stage V2 (filetests on `jit.q32` and `rv32.q32`) — correctness baseline
- Stage V1 (object + emulator in `lpvm-cranelift`)

## Estimated scope

~400–600 lines of feature gating, `CompileOptions` expansion, and memory
strategy work. Plus Cranelift dependency flag plumbing.

## Firmware validation (embedded JIT)

On-device GLSL → JIT is exercised by `fw-tests` (`scene_render_emu`, `alloc_trace_emu`) and by
`cargo check -p fw-esp32` on `riscv32imac-unknown-none-elf` with `server`. See
`docs/plans-done/2026-03-26-fw-embedded-shader-jit/00-notes.md` for the copy-paste acceptance
commands.
