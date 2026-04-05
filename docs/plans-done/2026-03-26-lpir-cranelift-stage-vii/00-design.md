# Design: lpir-cranelift Stage VII — Delete Old Compiler

Roadmap: [stage-vii-cleanup.md](../../roadmaps-old/2026-03-24-lpir-cranelift/stage-vii-cleanup.md)
Notes: [00-notes.md](./00-notes.md)

## Scope

Delete the entire old `lps-cranelift` compiler chain and all code that
only existed to support it. Acceptance criteria: `lps-frontend` is fully
removed from the tree.

## File structure

```
DELETED (crate directories):
lp-shader/
├── lps-cranelift/              # Old AST→CLIF compiler
├── lps-jit-util/               # Old JIT calling convention utility
├── lps-frontend/               # Old GLSL parser / semantic analysis
├── esp32-glsl-jit/                 # Pre-lp-fw ESP32 test binary
├── lps-q32-metrics-app/        # Q32 precision metrics tool (old compiler)
Dockerfile.rv32-jit                 # Docker build for esp32-glsl-jit

UPDATED:
lp-shader/
├── lps-builtins-gen-app/
│   ├── Cargo.toml                  # DROP: lps-frontend dep
│   └── src/
│       ├── main.rs                 # REMOVE: registry.rs, mapping.rs, lpfx_fns.rs gen paths
│       └── lpfx/
│           ├── glsl_parse.rs       # INLINE: FunctionSignature / extract_function_signature
│           ├── validate.rs         # USE: local types (inlined)
│           └── generate.rs         # USE: local types (inlined)
├── lps-filetests/
│   ├── Cargo.toml                  # DROP: lps-frontend dep
│   └── src/test_run/test_glsl.rs   # USE: glsl::parser::Parse directly
│   └── tests/lpfx_builtins_memory.rs  # UN-IGNORE, test, update comment if needed
Cargo.toml                          # REMOVE: members, default-members, profiles
justfile                            # REMOVE: rv32_packages, build/test/clippy refs
scripts/lp-build.sh                 # CLEAN UP or DELETE
.idea/lp2025.iml                    # REMOVE: source folder entries
README.md                           # UPDATE: crate table
lp-shader/README.md                   # UPDATE: crate list
AGENTS.md                           # UPDATE: remove old crate references
.cursor/rules/no-std-compile-path.mdc  # UPDATE: remove confusion warning
```

## Architecture

```
DELETED (old chain):

  GLSL ──► glsl crate ──► lps-frontend ──► lps-cranelift ──► machine code
                                                    │
                                               lps-jit-util
                                                    │
                                               esp32-glsl-jit (test binary)

REMAINS (new chain — no changes):

  GLSL ──► naga (lps-frontend) ──► LPIR ──► lpir-cranelift ──► machine code
                                                 │
                                         lps-builtins-emu-app (ELF for rv32 emu)

  lps-builtins-gen-app: generates into lpir-cranelift, builtins, wasm, emu-app, builtin-ids
    (old-backend generation paths removed; lps-frontend types inlined as local structs)
```

## Main components

| Component              | Action                                                                                                                                                                                                                            |
|------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `lps-cranelift`        | Delete directory                                                                                                                                                                                                                  |
| `lps-jit-util`         | Delete directory                                                                                                                                                                                                                  |
| `lps-frontend`         | Delete directory (after migrating gen-app + filetests)                                                                                                                                                                            |
| `esp32-glsl-jit`       | Delete directory                                                                                                                                                                                                                  |
| `lps-q32-metrics-app`  | Delete directory                                                                                                                                                                                                                  |
| `lps-builtins-gen-app` | Remove old-backend gen paths; inline `FunctionSignature`, `Type`, `ParamQualifier`, `Parameter`, `extract_function_signature` as local types; drop `lps-frontend` dep; stop generating `lpfx_fns.rs`, `registry.rs`, `mapping.rs` |
| `lps-filetests`        | Replace `CompilationPipeline::parse()` with `TranslationUnit::parse()`; drop `lps-frontend` dep                                                                                                                                   |
| Workspace `Cargo.toml` | Remove deleted crates from `members`, `default-members`, profile entries                                                                                                                                                          |
| `justfile`             | Remove deleted crate references from `rv32_packages`, build/test/clippy                                                                                                                                                           |
| Scripts / Docker       | Delete `Dockerfile.rv32-jit`; clean up `scripts/lp-build.sh`                                                                                                                                                                      |
| Docs                   | Update `README.md`, `lp-shader/README.md`, `AGENTS.md`, cursor rules; leave historical docs                                                                                                                                       |
| Ignored tests          | Un-ignore `lpfx_builtins_memory`, re-ignore with updated comment if WASM ABI still broken                                                                                                                                         |

## Decisions (from notes)

- Q1: Delete `lps-frontend` entirely; inline types into gen-app.
- Q2: Delete `lps-q32-metrics-app`.
- Q3: Full cleanup — acceptance is `lps-frontend` gone.
- Q4: Delete `Dockerfile.rv32-jit`, `scripts/lp-build.sh`, `esp32-glsl-jit`.
- Q5: Un-ignore test, re-ignore with updated comment if still failing.
- Q6: Leave historical docs; update living docs.
