# lps post-refactor audit — crates, READMEs, dependencies

**Date:** 2026-03-26  
**Scope:** `lp-shader/` after removal of legacy compiler crates; root `README.md`, `AGENTS.md` (
shader
stack mentions), and workspace layout vs documentation.

## Purpose

Record the **current crate set**, whether **documentation matches the implementation**, and whether
the **dependency graph** is coherent for the embedded GLSL → LPIR → Cranelift path and for host
tooling (filetests, WASM preview).

## Crate inventory

All packages live under `lp-shader/` unless noted. Workspace membership follows the root
`Cargo.toml`
`[workspace].members` plus Cargo’s inclusion of path crates that use workspace inheritance (see
§Workspace membership).

| Crate                   | Role (from code / `Cargo.toml`)                                             | README present             |
|-------------------------|-----------------------------------------------------------------------------|----------------------------|
| `lps-builtin-ids`       | Generated `BuiltinId` and GLSL name mapping (written by gen app)            | No                         |
| `lps-builtins`          | `#[no_mangle]` builtins (Q32 / f32), LPFX; links `lpfx-impl-macro`          | Yes (stale — see findings) |
| `lps-builtins-gen-app`  | Scans `lps-builtins`, emits IDs, `generated_builtin_abi.rs`, refs, etc.     | Yes (stale paths)          |
| `lps-builtins-emu-app`  | RISC-V guest binary: links all builtins for emu / filetests                 | Yes (stale names)          |
| `lps-builtins-wasm`     | `cdylib` WASM builtins (`import-memory`)                                    | Yes (accurate)             |
| `lps-shared`            | Shared type / function-signature shapes (`#![no_std]` + alloc)              | No                         |
| `lps-diagnostics`       | `GlslError`, spans, codes                                                   | No                         |
| `lps-exec`              | `GlslExecutable` + glue for filetests backends                              | No                         |
| `lpvm`                  | Runtime values / literals; uses `glsl` parser fork                          | No                         |
| `lps-frontend`          | GLSL → LPIR via **naga** `glsl-in`                                          | No                         |
| `lpir`                  | LPIR IR (`IrModule`, types, ops)                                            | No                         |
| `lpvm-cranelift`        | LPIR → Cranelift → JIT / object; optional `lps-frontend` via `glsl` feature | No                         |
| `lps-filetests`         | Corpus + harness (JIT / WASM / RV32)                                        | Yes (current)              |
| `lps-filetests-app`     | CLI runner for filetests                                                    | No                         |
| `lps-filetests-gen-app` | Generates repetitive `.glsl` tests                                          | No                         |
| `lps-wasm`              | GLSL → WASM (Naga → LPIR → emit)                                            | Yes (stale architecture)   |
| `lpfx-impl-macro`       | Proc-macros for LPFX builtins                                               | No                         |

**Not under `lp-shader/` but part of the same pipeline:** `lp-core/lp-engine` → `lpvm-cranelift` (+
`lps-builtins`); `lp-riscv/*` for RV32 filetests.

## Dependency graph (conceptual)

Solid arrows are normal dependencies; dashed lines are optional or “tooling only”.

```mermaid
flowchart TB
  subgraph frontend_ir["Frontend + IR"]
    ids["lps-builtin-ids"]
    lpir["lpir"]
    naga["lps-frontend"]
    naga --> ids
    naga --> lpir
  end

  subgraph support["Shared helpers"]
    diag["lps-diagnostics"]
    core["lps-shared"]
    values["lpvm"]
    values --> diag
    values -.-> glsl_parser["glsl fork"]
  end

  subgraph builtins["Builtins"]
    pm["lpfx-impl-macro"]
    builtins["lps-builtins"]
    builtins --> pm
  end

  subgraph codegen["Codegen"]
    cf["lpvm-cranelift"]
    cf --> ids
    cf --> builtins
    cf --> lpir
    cf -.-> naga
  end

  subgraph wasm["WASM preview"]
    wasm["lps-wasm"]
    wasm --> ids
    wasm --> naga
    wasm --> lpir
  end

  subgraph exec_tests["Execution / tests"]
    exec["lps-exec"]
    exec --> core
    exec --> diag
    exec --> values
    ft["lps-filetests"]
    ft --> exec
    ft --> naga
    ft --> lpir
    ft --> cf
    ft --> wasm
    ft -.-> glsl_parser
    ft --> riscv["lp-riscv-*"]
  end

  engine["lp-engine"] --> cf
  engine --> builtins
```

**Observations:**

- **On-device compile path** (`lp-engine`): `lpvm-cranelift` with `glsl` → `lps-frontend` → `lpir`;
  builtins from `lps-builtins`. No `lps-exec`, `lpvm`, or `glsl` parser crate on that
  path — appropriate for splitting “compiler” vs “test harness helpers.”
- **`lps-shared`**: Used by `lps-exec` and `lps-filetests` only. The crate-level doc
  comment in `lps-shared/src/lib.rs` says it is used by `lps-frontend`; **that is not true** in
  the current `Cargo.toml` graph (naga crate has no `lps-shared` dependency).
- **`lpvm-cranelift` `package.description`** still says “Experimental … (Stage II)”; the stack is
  now production for firmware — consider updating the string to avoid implying a spike.

## README and top-level doc alignment

### `lp-shader/README.md`

- Accurately lists the new crates (`lps-shared`, `lps-diagnostics`, `lpvm`,
  `lps-exec`, `lps-wasm`) and the Naga → LPIR → Cranelift story.
- Commands are generally valid from repo root (`./scripts/glsl-filetests.sh`,
  `cargo check -p fw-esp32 …`).
- Minor nit: `cargo build` from “inside `lps`” without `-p` is ambiguous; the workspace root is
  the repo root — prefer `cargo build` from root or `cargo build -p <crate>`.

### Root `README.md` — “GLSL Compiler (`lp-shader/`)” section

Compared to the actual workspace crates, the following **are implemented but not listed** in that
bullet list:

- `lps-shared`
- `lps-diagnostics`
- `lpvm`
- `lps-exec`
- `lps-wasm`

Readers scanning the repo structure will miss the WASM backend and the shared “new stack” types
unless they open `lp-shader/README.md`.

### `AGENTS.md`

- Architecture diagram (GLSL → `lps-frontend` → LPIR → `lpvm-cranelift`) matches the current product
  path.
- The “Key Crates” table is intentionally minimal; no change required unless you want parity with
  root `README.md`.

### Per-crate README quality

| README                           | Issue                                                                                                                                                                                                                                                                                                                                     |
|----------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `lps-builtins/README.md`         | References **removed** `lps-compiler`, wrong paths (`crates/`, `lightplayer/`), and “registry in lps-compiler”. Builtin registration is now via **`lps-builtins-gen-app`** → `lps-builtin-ids` + `lpvm-cranelift/src/generated_builtin_abi.rs`.                                                                                           |
| `lps-builtins-gen-app/README.md` | Still describes outputs like `registry.rs`, `backend/builtins/`, and paths under `crates/lps-builtins`. Actual generator writes **`lps-builtin-ids`**, **`generated_builtin_abi.rs`**, **`builtin_refs.rs`**, etc., under `lp-shader/…`.                                                                                                  |
| `lps-builtins-emu-app/README.md` | References **`lps-compiler`** and **`lp-filetests`**; should reference **`lps-filetests`** / RV32 harness.                                                                                                                                                                                                                                |
| `lps-wasm/README.md`             | Architecture still describes **`lps-frontend`** and AST tree-walk; **`lib.rs`** documents **Naga → LPIR → WASM** (`emit/`, not the old `codegen/` tree). “Why not Cranelift” rationale is partly historical; much of the “Key design decisions” may still apply to the emitter, but the **pipeline diagram and module layout are wrong**. |
| `lps-filetests/README.md`        | Matches current scripts (`scripts/glsl-filetests.sh`, `just test-filetests`) and backend story.                                                                                                                                                                                                                                           |
| `lps-builtins-wasm/README.md`    | Consistent with `justfile` / `cargo build -p lps-builtins-wasm`.                                                                                                                                                                                                                                                                          |

### `scripts/build-builtins.sh` vs READMEs

Several READMEs say to run **`scripts/build-builtins.sh`**. The script’s **hash inputs** still point
at removed layout paths (`lp-shader/apps/…`, `lp-shader/crates/…`). The **build** portion (
`cd "$LIGHTPLAYER_DIR"` + `cargo build -p lps-builtins-emu-app`) still matches the current crate
names. Risk: **incremental “skip codegen” may be wrong** because the watched directories may be
empty or wrong. Worth fixing in a follow-up (not required for dependency correctness, but affects
trust in docs).

## Workspace membership

- **`lpfx-impl-macro`** is **not** listed in the explicit `[workspace].members` array in the root
  `Cargo.toml`, but **`cargo metadata` includes it** in `workspace_members` (as a path dependency of
  `lps-builtins` using `version.workspace = true`). This is easy to miss when editing the
  workspace list.
- **Recommendation:** Add `"lp-shader/lpfx-impl-macro"` explicitly to `[workspace].members` so the
  manifest matches tooling and onboarding expectations.

## Root `README.md` acknowledgments

- **`glsl-parser` fork** remains relevant: `lpvm` and `lps-filetests` depend on the
  `glsl` crate (fork with spans).
- **Naga** is the primary GLSL front end for **`lps-frontend`**; the acknowledgments section does
  not mention Naga — optional doc improvement.

## Recommendations (prioritized) -- all resolved

1. ~~**Rewrite** stale
   READMEs (`lps-builtins`, `lps-builtins-gen-app`, `lps-builtins-emu-app`,
   `lps-wasm`).~~ Done.
2. ~~**Extend** root `README.md` GLSL section with five missing crates.~~ Done (bullets + link to
   `lp-shader/README.md`).
3. ~~**Fix** `lps-shared` crate docs (`lps-frontend` claim).~~ Done.
4. ~~**Refresh** `lpvm-cranelift` `description` in `Cargo.toml`.~~ Done.
5. ~~**Repair** `scripts/build-builtins.sh` hash paths.~~ Done.
6. ~~**Add** `lp-shader/CRATES.md` + per-crate READMEs.~~ Done.
7. ~~**Add** Naga and pp-rs to acknowledgments.~~ Done (separate bullets).
8. ~~**Overhaul** `lpir/README.md` -- rationale, anti-corruption layer, examples, doc index.~~ Done.
9. ~~**Update** `docs/lpir/00-overview.md` -- Cranelift no longer "planned", pipeline shows
   interpreter, crate layout current.~~ Done.

## Conclusion

The **dependency graph is coherent**: firmware and `lp-engine` pull *
*`lpvm-cranelift` + `lps-frontend` + `lpir` + builtins** without pulling the filetest-only `glsl`
parser crate. Documentation now matches the post-refactor crate set across READMEs, the root README,
`CRATES.md`, `AGENTS.md`, the `docs/lpir/` spec, and acknowledgments.
