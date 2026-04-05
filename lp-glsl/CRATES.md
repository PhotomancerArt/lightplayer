# `lp-glsl` crates (quick reference)

Overview of this directory lives in [`README.md`](README.md). Paths are relative to `lp-glsl/`.

| Crate                                                     | One-line role                                                               |
|-----------------------------------------------------------|-----------------------------------------------------------------------------|
| [`lpir`](lpir/)                                           | LightPlayer IR: types, ops, `IrModule` (`no_std` + alloc).                  |
| [`lp-glsl-naga`](lp-glsl-naga/)                           | GLSL source → LPIR via **naga** `glsl-in`.                                  |
| [`lpir-cranelift`](lpir-cranelift/)                       | LPIR → Cranelift → JIT / object (RISC-V); optional `glsl` → `lp-glsl-naga`. |
| [`lp-glsl-wasm`](lp-glsl-wasm/)                           | LPIR → WASM (`wasm-encoder`) for browser / `wasm.q32` filetests.            |
| [`lp-glsl-builtin-ids`](lp-glsl-builtin-ids/)             | `BuiltinId` enum and mappings (**generated**; do not hand-edit).            |
| [`lp-glsl-builtins`](lp-glsl-builtins/)                   | `extern "C"` builtin implementations (Q32/f32, LPFX).                       |
| [`lpfx-impl-macro`](lpfx-impl-macro/)                     | Proc-macros for LPFX builtin definitions.                                   |
| [`lp-glsl-builtins-gen-app`](lp-glsl-builtins-gen-app/)   | Scans builtins; emits IDs, ABI, refs, `mod.rs`, WASM import types.          |
| [`lp-glsl-builtins-emu-app`](lp-glsl-builtins-emu-app/)   | RV32 guest linking all builtins (emulator filetests).                       |
| [`lp-glsl-builtins-wasm`](lp-glsl-builtins-wasm/)         | WASM `cdylib` of builtins (`import-memory`).                                |
| [`lps-types`](lps-types/)                                 | Shared GLSL type / function-signature shapes (no parser).                   |
| [`lp-glsl-diagnostics`](lp-glsl-diagnostics/)             | `GlslError`, codes, source locations.                                       |
| [`lp-glsl-abi`](lp-glsl-abi/)                             | Runtime values and literal parsing (`glsl` fork).                           |
| [`lp-glsl-exec`](lp-glsl-exec/)                           | `GlslExecutable` trait; filetest / runner glue.                             |
| [`lp-glsl-filetests`](lp-glsl-filetests/)                 | GLSL filetest corpus and harness (JIT / WASM / RV32).                       |
| [`lp-glsl-filetests-app`](lp-glsl-filetests-app/)         | CLI to run filetests.                                                       |
| [`lp-glsl-filetests-gen-app`](lp-glsl-filetests-gen-app/) | Generates repetitive `.glsl` tests under `filetests/`.                      |

**Dependency spine (firmware):** `lp-glsl-naga` → `lpir` ← `lpir-cranelift` ← `lp-engine`;
`lp-glsl-builtins` + `lp-glsl-builtin-ids` alongside codegen.

**Test-only / host helpers:** `lp-glsl-exec`, `lp-glsl-abi`, `lps-types`, `lp-glsl-diagnostics`,
`lp-glsl-filetests*`, `lp-glsl-wasm` (as used by filetests and web demo).
