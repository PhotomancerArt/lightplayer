# GLSL → WASM builtins (continuation): path to Rainbow

**Predecessor:** `docs/plans-done/2026-03-18-glsl-wasm-builtins/` — `builtins.wasm`, Q32 import
plumbing, inline gentype builtins, `q32_builtin_link` smoke test.

**Goal:** `examples/basic/src/rainbow.shader/main.glsl` compiles with `glsl_wasm` Q32, links with
`lps_builtins_wasm.wasm` + shared `env.memory`, and runs under wasmtime.

| File                                                             | Purpose                                                          |
|------------------------------------------------------------------|------------------------------------------------------------------|
| [`00-notes.md`](00-notes.md)                                     | Scope, current state, questions + decisions.                     |
| [`00-design.md`](00-design.md)                                   | Architecture, file structure, component summary.                 |
| [`01-fix-psrdnoise-seed.md`](01-fix-psrdnoise-seed.md)           | Phase 1: Fix psrdnoise seed parameter bug.                       |
| [`02-math-gaps.md`](02-math-gaps.md)                             | Phase 2: Inline `floor`, `fract` Q32; verify `atan`/`cos`/`exp`. |
| [`03-lpfn-call-emission.md`](03-lpfn-call-emission.md)           | Phase 3: LPFX FunCall dispatch, arg flattening, out params.      |
| [`04-filetest-runner-rainbow.md`](04-filetest-runner-rainbow.md) | Phase 4: Filetest runner linking, Rainbow end-to-end.            |
| [`05-cleanup-validation.md`](05-cleanup-validation.md)           | Phase 5: Cleanup, validation, plan closure.                      |

Design references (archived): `docs/plans-done/2026-03-18-glsl-wasm-builtins/00-design.md`,
`00-notes.md`.
