# Summary: embedded GLSL JIT (`fw-emu` / `fw-esp32`)

## What shipped

- **`lpir-cranelift`:** Split **`glsl`** (front end / `lp-glsl-naga`) from **`std`** (host: `cranelift-native`, etc.). **`jit()`** and **`CompilerError::Lower`** use **`glsl`**. Default features **`std` + `glsl`**. RISC-V32 lowers multi-return functions with **StructReturn** when **`return_types.len() > 2`**; invoke path uses a hidden buffer pointer on **`riscv32`** only.
- **`lp-engine`:** Always enables **`glsl`** on **`lpir-cranelift`**; **`ShaderRuntime`** uses real **`compile_shader`** / **`render`** without **`std`**.
- **`lp-riscv-emu` `test_util`:** Removed stale binary short-circuit so **`ensure_binary_built`** always runs **`cargo build`** (cache key ignored sources).
- **Docs:** Plan **`00-notes`** acceptance commands; roadmap VI-A cross-link.

## Key files

- `lp-glsl/lpir-cranelift/Cargo.toml`, `src/compile.rs`, `src/lib.rs`, `src/error.rs`, `src/emit/mod.rs`, `src/emit/call.rs`, `src/module_lower.rs`, `src/jit_module.rs`, `src/direct_call.rs`, `src/invoke.rs`, `src/call.rs`, `src/emu_run.rs`
- `lp-core/lp-engine/Cargo.toml`, `src/nodes/shader/runtime.rs`
- `lp-riscv/lp-riscv-emu/src/test_util.rs`
- `docs/plans/2026-03-26-fw-embedded-shader-jit/00-notes.md`, `docs/roadmaps/2026-03-24-lpir-cranelift/stage-vi-a-embedded-readiness.md`

## Follow-ups

- Optional **I-cache fence** (`fence.i`) after JIT finalize on real ESP32 if required by silicon (see `jit_memory` / Cranelift finalize).
- **`pub use signature_for_ir_func`** now requires **`pointer_type`** and **`TargetIsa`**; external callers should build an ISA (same as **`module_lower`**).
