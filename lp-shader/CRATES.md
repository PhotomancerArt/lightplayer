# `lps` crates (quick reference)

Overview of this directory lives in [`README.md`](README.md). Paths are relative to `lp-shader/`.

| Crate                                                 | One-line role                                                               |
|-------------------------------------------------------|-----------------------------------------------------------------------------|
| [`lpir`](lpir/)                                       | LightPlayer IR: types, ops, `IrModule` (`no_std` + alloc).                  |
| [`lps-naga`](lps-naga/)                           | GLSL source → LPIR via **naga** `glsl-in`.                                  |
| [`lpir-cranelift`](lpir-cranelift/)                   | LPIR → Cranelift → JIT / object (RISC-V); optional `glsl` → `lps-naga`. |
| [`lps-wasm`](lps-wasm/)                       | LPIR → WASM (`wasm-encoder`) for browser / `wasm.q32` filetests.            |
| [`lps-builtin-ids`](lps-builtin-ids/)             | `BuiltinId` enum and mappings (**generated**; do not hand-edit).            |
| [`lps-builtins`](lps-builtins/)                   | `extern "C"` builtin implementations (Q32/f32, LPFX).                       |
| [`lpfx-impl-macro`](lpfx-impl-macro/)                 | Proc-macros for LPFX builtin definitions.                                   |
| [`lps-builtins-gen-app`](lps-builtins-gen-app/)   | Scans builtins; emits IDs, ABI, refs, `mod.rs`, WASM import types.          |
| [`lps-builtins-emu-app`](lps-builtins-emu-app/)   | RV32 guest linking all builtins (emulator filetests).                       |
| [`lps-builtins-wasm`](lps-builtins-wasm/)         | WASM `cdylib` of builtins (`import-memory`).                                |
| [`lps-shared`](lps-shared/)                           | Shared GLSL type / function-signature shapes (no parser).                   |
| [`lps-diagnostics`](lps-diagnostics/)             | `GlslError`, codes, source locations.                                       |
| [`lpvm`](lpvm/)                                       | Runtime values and literal parsing (`glsl` fork).                           |
| [`lps-exec`](lps-exec/)                       | `GlslExecutable` trait; filetest / runner glue.                             |
| [`lps-filetests`](lps-filetests/)                 | GLSL filetest corpus and harness (JIT / WASM / RV32).                       |
| [`lps-filetests-app`](lps-filetests-app/)         | CLI to run filetests.                                                       |
| [`lps-filetests-gen-app`](lps-filetests-gen-app/) | Generates repetitive `.glsl` tests under `filetests/`.                      |

**Dependency spine (firmware):** `lps-naga` → `lpir` ← `lpir-cranelift` ← `lp-engine`;
`lps-builtins` + `lps-builtin-ids` alongside codegen.

**Test-only / host helpers:** `lps-exec`, `lpvm`, `lps-shared`, `lps-diagnostics`,
`lps-filetests*`, `lps-wasm` (as used by filetests and web demo).
