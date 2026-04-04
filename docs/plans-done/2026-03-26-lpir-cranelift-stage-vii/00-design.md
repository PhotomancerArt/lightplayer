# Design: lpir-cranelift Stage VII — Delete Old Compiler

Roadmap: [stage-vii-cleanup.md](../../roadmaps-old/2026-03-24-lpir-cranelift/stage-vii-cleanup.md)
Notes: [00-notes.md](./00-notes.md)

## Scope

Delete the entire old `lp-glsl-cranelift` compiler chain and all code that
only existed to support it. Acceptance criteria: `lp-glsl-frontend` is fully
removed from the tree.

## File structure

```
DELETED (crate directories):
lp-glsl/
├── lp-glsl-cranelift/              # Old AST→CLIF compiler
├── lp-glsl-jit-util/               # Old JIT calling convention utility
├── lp-glsl-frontend/               # Old GLSL parser / semantic analysis
├── esp32-glsl-jit/                 # Pre-lp-fw ESP32 test binary
├── lp-glsl-q32-metrics-app/        # Q32 precision metrics tool (old compiler)
Dockerfile.rv32-jit                 # Docker build for esp32-glsl-jit

UPDATED:
lp-glsl/
├── lp-glsl-builtins-gen-app/
│   ├── Cargo.toml                  # DROP: lp-glsl-frontend dep
│   └── src/
│       ├── main.rs                 # REMOVE: registry.rs, mapping.rs, lpfx_fns.rs gen paths
│       └── lpfx/
│           ├── glsl_parse.rs       # INLINE: FunctionSignature / extract_function_signature
│           ├── validate.rs         # USE: local types (inlined)
│           └── generate.rs         # USE: local types (inlined)
├── lp-glsl-filetests/
│   ├── Cargo.toml                  # DROP: lp-glsl-frontend dep
│   └── src/test_run/test_glsl.rs   # USE: glsl::parser::Parse directly
│   └── tests/lpfx_builtins_memory.rs  # UN-IGNORE, test, update comment if needed
Cargo.toml                          # REMOVE: members, default-members, profiles
justfile                            # REMOVE: rv32_packages, build/test/clippy refs
scripts/lp-build.sh                 # CLEAN UP or DELETE
.idea/lp2025.iml                    # REMOVE: source folder entries
README.md                           # UPDATE: crate table
lp-glsl/README.md                   # UPDATE: crate list
AGENTS.md                           # UPDATE: remove old crate references
.cursor/rules/no-std-compile-path.mdc  # UPDATE: remove confusion warning
```

## Architecture

```
DELETED (old chain):

  GLSL ──► glsl crate ──► lp-glsl-frontend ──► lp-glsl-cranelift ──► machine code
                                                    │
                                               lp-glsl-jit-util
                                                    │
                                               esp32-glsl-jit (test binary)

REMAINS (new chain — no changes):

  GLSL ──► naga (lp-glsl-naga) ──► LPIR ──► lpir-cranelift ──► machine code
                                                 │
                                         lp-glsl-builtins-emu-app (ELF for rv32 emu)

  lp-glsl-builtins-gen-app: generates into lpir-cranelift, builtins, wasm, emu-app, builtin-ids
    (old-backend generation paths removed; lp-glsl-frontend types inlined as local structs)
```

## Main components

| Component                  | Action                                                                                                                                                                                                                                |
|----------------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `lp-glsl-cranelift`        | Delete directory                                                                                                                                                                                                                      |
| `lp-glsl-jit-util`         | Delete directory                                                                                                                                                                                                                      |
| `lp-glsl-frontend`         | Delete directory (after migrating gen-app + filetests)                                                                                                                                                                                |
| `esp32-glsl-jit`           | Delete directory                                                                                                                                                                                                                      |
| `lp-glsl-q32-metrics-app`  | Delete directory                                                                                                                                                                                                                      |
| `lp-glsl-builtins-gen-app` | Remove old-backend gen paths; inline `FunctionSignature`, `Type`, `ParamQualifier`, `Parameter`, `extract_function_signature` as local types; drop `lp-glsl-frontend` dep; stop generating `lpfx_fns.rs`, `registry.rs`, `mapping.rs` |
| `lp-glsl-filetests`        | Replace `CompilationPipeline::parse()` with `TranslationUnit::parse()`; drop `lp-glsl-frontend` dep                                                                                                                                   |
| Workspace `Cargo.toml`     | Remove deleted crates from `members`, `default-members`, profile entries                                                                                                                                                              |
| `justfile`                 | Remove deleted crate references from `rv32_packages`, build/test/clippy                                                                                                                                                               |
| Scripts / Docker           | Delete `Dockerfile.rv32-jit`; clean up `scripts/lp-build.sh`                                                                                                                                                                          |
| Docs                       | Update `README.md`, `lp-glsl/README.md`, `AGENTS.md`, cursor rules; leave historical docs                                                                                                                                             |
| Ignored tests              | Un-ignore `lpfx_builtins_memory`, re-ignore with updated comment if WASM ABI still broken                                                                                                                                             |

## Decisions (from notes)

- Q1: Delete `lp-glsl-frontend` entirely; inline types into gen-app.
- Q2: Delete `lp-glsl-q32-metrics-app`.
- Q3: Full cleanup — acceptance is `lp-glsl-frontend` gone.
- Q4: Delete `Dockerfile.rv32-jit`, `scripts/lp-build.sh`, `esp32-glsl-jit`.
- Q5: Un-ignore test, re-ignore with updated comment if still failing.
- Q6: Leave historical docs; update living docs.
