# lp-glsl post-refactor audit — crates, READMEs, dependencies

**Date:** 2026-03-26  
**Scope:** `lp-glsl/` after removal of legacy compiler crates; root `README.md`, `AGENTS.md` (shader
stack mentions), and workspace layout vs documentation.

## Purpose

Record the **current crate set**, whether **documentation matches the implementation**, and whether
the **dependency graph** is coherent for the embedded GLSL → LPIR → Cranelift path and for host
tooling (filetests, WASM preview).

## Crate inventory

All packages live under `lp-glsl/` unless noted. Workspace membership follows the root `Cargo.toml`
`[workspace].members` plus Cargo’s inclusion of path crates that use workspace inheritance (see
§Workspace membership).

| Crate                       | Role (from code / `Cargo.toml`)                                             | README present             |
|-----------------------------|-----------------------------------------------------------------------------|----------------------------|
| `lp-glsl-builtin-ids`       | Generated `BuiltinId` and GLSL name mapping (written by gen app)            | No                         |
| `lp-glsl-builtins`          | `#[no_mangle]` builtins (Q32 / f32), LPFX; links `lpfx-impl-macro`          | Yes (stale — see findings) |
| `lp-glsl-builtins-gen-app`  | Scans `lp-glsl-builtins`, emits IDs, `generated_builtin_abi.rs`, refs, etc. | Yes (stale paths)          |
| `lp-glsl-builtins-emu-app`  | RISC-V guest binary: links all builtins for emu / filetests                 | Yes (stale names)          |
| `lp-glsl-builtins-wasm`     | `cdylib` WASM builtins (`import-memory`)                                    | Yes (accurate)             |
| `lp-glsl-core`              | Shared type / function-signature shapes (`#![no_std]` + alloc)              | No                         |
| `lp-glsl-diagnostics`       | `GlslError`, spans, codes                                                   | No                         |
| `lp-glsl-exec`              | `GlslExecutable` + glue for filetests backends                              | No                         |
| `lp-glsl-abi`               | Runtime values / literals; uses `glsl` parser fork                          | No                         |
| `lp-glsl-naga`              | GLSL → LPIR via **naga** `glsl-in`                                          | No                         |
| `lpir`                      | LPIR IR (`IrModule`, types, ops)                                            | No                         |
| `lpir-cranelift`            | LPIR → Cranelift → JIT / object; optional `lp-glsl-naga` via `glsl` feature | No                         |
| `lp-glsl-filetests`         | Corpus + harness (JIT / WASM / RV32)                                        | Yes (current)              |
| `lp-glsl-filetests-app`     | CLI runner for filetests                                                    | No                         |
| `lp-glsl-filetests-gen-app` | Generates repetitive `.glsl` tests                                          | No                         |
| `lp-glsl-wasm`              | GLSL → WASM (Naga → LPIR → emit)                                            | Yes (stale architecture)   |
| `lpfx-impl-macro`           | Proc-macros for LPFX builtins                                               | No                         |

**Not under `lp-glsl/` but part of the same pipeline:** `lp-core/lp-engine` → `lpir-cranelift` (+
`lp-glsl-builtins`); `lp-riscv/*` for RV32 filetests.

## Dependency graph (conceptual)

Solid arrows are normal dependencies; dashed lines are optional or “tooling only”.

```mermaid
flowchart TB
  subgraph frontend_ir["Frontend + IR"]
    ids["lp-glsl-builtin-ids"]
    lpir["lpir"]
    naga["lp-glsl-naga"]
    naga --> ids
    naga --> lpir
  end

  subgraph support["Shared helpers"]
    diag["lp-glsl-diagnostics"]
    core["lp-glsl-core"]
    values["lp-glsl-abi"]
    values --> diag
    values -.-> glsl_parser["glsl fork"]
  end

  subgraph builtins["Builtins"]
    pm["lpfx-impl-macro"]
    builtins["lp-glsl-builtins"]
    builtins --> pm
  end

  subgraph codegen["Codegen"]
    cf["lpir-cranelift"]
    cf --> ids
    cf --> builtins
    cf --> lpir
    cf -.-> naga
  end

  subgraph wasm["WASM preview"]
    wasm["lp-glsl-wasm"]
    wasm --> ids
    wasm --> naga
    wasm --> lpir
  end

  subgraph exec_tests["Execution / tests"]
    exec["lp-glsl-exec"]
    exec --> core
    exec --> diag
    exec --> values
    ft["lp-glsl-filetests"]
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

- **On-device compile path** (`lp-engine`): `lpir-cranelift` with `glsl` → `lp-glsl-naga` → `lpir`;
  builtins from `lp-glsl-builtins`. No `lp-glsl-exec`, `lp-glsl-abi`, or `glsl` parser crate on that
  path — appropriate for splitting “compiler” vs “test harness helpers.”
- **`lp-glsl-core`**: Used by `lp-glsl-exec` and `lp-glsl-filetests` only. The crate-level doc
  comment in `lp-glsl-core/src/lib.rs` says it is used by `lp-glsl-naga`; **that is not true** in
  the current `Cargo.toml` graph (naga crate has no `lp-glsl-core` dependency).
- **`lpir-cranelift` `package.description`** still says “Experimental … (Stage II)”; the stack is
  now production for firmware — consider updating the string to avoid implying a spike.

## README and top-level doc alignment

### `lp-glsl/README.md`

- Accurately lists the new crates (`lp-glsl-core`, `lp-glsl-diagnostics`, `lp-glsl-abi`,
  `lp-glsl-exec`, `lp-glsl-wasm`) and the Naga → LPIR → Cranelift story.
- Commands are generally valid from repo root (`./scripts/glsl-filetests.sh`,
  `cargo check -p fw-esp32 …`).
- Minor nit: `cargo build` from “inside `lp-glsl`” without `-p` is ambiguous; the workspace root is
  the repo root — prefer `cargo build` from root or `cargo build -p <crate>`.

### Root `README.md` — “GLSL Compiler (`lp-glsl/`)” section

Compared to the actual workspace crates, the following **are implemented but not listed** in that
bullet list:

- `lp-glsl-core`
- `lp-glsl-diagnostics`
- `lp-glsl-abi`
- `lp-glsl-exec`
- `lp-glsl-wasm`

Readers scanning the repo structure will miss the WASM backend and the shared “new stack” types
unless they open `lp-glsl/README.md`.

### `AGENTS.md`

- Architecture diagram (GLSL → `lp-glsl-naga` → LPIR → `lpir-cranelift`) matches the current product
  path.
- The “Key Crates” table is intentionally minimal; no change required unless you want parity with
  root `README.md`.

### Per-crate README quality

| README                               | Issue                                                                                                                                                                                                                                                                                                                                         |
|--------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `lp-glsl-builtins/README.md`         | References **removed** `lp-glsl-compiler`, wrong paths (`crates/`, `lightplayer/`), and “registry in lp-glsl-compiler”. Builtin registration is now via **`lp-glsl-builtins-gen-app`** → `lp-glsl-builtin-ids` + `lpir-cranelift/src/generated_builtin_abi.rs`.                                                                               |
| `lp-glsl-builtins-gen-app/README.md` | Still describes outputs like `registry.rs`, `backend/builtins/`, and paths under `crates/lp-glsl-builtins`. Actual generator writes **`lp-glsl-builtin-ids`**, **`generated_builtin_abi.rs`**, **`builtin_refs.rs`**, etc., under `lp-glsl/…`.                                                                                                |
| `lp-glsl-builtins-emu-app/README.md` | References **`lp-glsl-compiler`** and **`lp-filetests`**; should reference **`lp-glsl-filetests`** / RV32 harness.                                                                                                                                                                                                                            |
| `lp-glsl-wasm/README.md`             | Architecture still describes **`lp-glsl-frontend`** and AST tree-walk; **`lib.rs`** documents **Naga → LPIR → WASM** (`emit/`, not the old `codegen/` tree). “Why not Cranelift” rationale is partly historical; much of the “Key design decisions” may still apply to the emitter, but the **pipeline diagram and module layout are wrong**. |
| `lp-glsl-filetests/README.md`        | Matches current scripts (`scripts/glsl-filetests.sh`, `just test-filetests`) and backend story.                                                                                                                                                                                                                                               |
| `lp-glsl-builtins-wasm/README.md`    | Consistent with `justfile` / `cargo build -p lp-glsl-builtins-wasm`.                                                                                                                                                                                                                                                                          |

### `scripts/build-builtins.sh` vs READMEs

Several READMEs say to run **`scripts/build-builtins.sh`**. The script’s **hash inputs** still point
at removed layout paths (`lp-glsl/apps/…`, `lp-glsl/crates/…`). The **build** portion (
`cd "$LIGHTPLAYER_DIR"` + `cargo build -p lp-glsl-builtins-emu-app`) still matches the current crate
names. Risk: **incremental “skip codegen” may be wrong** because the watched directories may be
empty or wrong. Worth fixing in a follow-up (not required for dependency correctness, but affects
trust in docs).

## Workspace membership

- **`lpfx-impl-macro`** is **not** listed in the explicit `[workspace].members` array in the root
  `Cargo.toml`, but **`cargo metadata` includes it** in `workspace_members` (as a path dependency of
  `lp-glsl-builtins` using `version.workspace = true`). This is easy to miss when editing the
  workspace list.
- **Recommendation:** Add `"lp-glsl/lpfx-impl-macro"` explicitly to `[workspace].members` so the
  manifest matches tooling and onboarding expectations.

## Root `README.md` acknowledgments

- **`glsl-parser` fork** remains relevant: `lp-glsl-abi` and `lp-glsl-filetests` depend on the
  `glsl` crate (fork with spans).
- **Naga** is the primary GLSL front end for **`lp-glsl-naga`**; the acknowledgments section does
  not mention Naga — optional doc improvement.

## Recommendations (prioritized) -- all resolved

1. ~~**Rewrite** stale
   READMEs (`lp-glsl-builtins`, `lp-glsl-builtins-gen-app`, `lp-glsl-builtins-emu-app`,
   `lp-glsl-wasm`).~~ Done.
2. ~~**Extend** root `README.md` GLSL section with five missing crates.~~ Done (bullets + link to
   `lp-glsl/README.md`).
3. ~~**Fix** `lp-glsl-core` crate docs (`lp-glsl-naga` claim).~~ Done.
4. ~~**Refresh** `lpir-cranelift` `description` in `Cargo.toml`.~~ Done.
5. ~~**Repair** `scripts/build-builtins.sh` hash paths.~~ Done.
6. ~~**Add** `lp-glsl/CRATES.md` + per-crate READMEs.~~ Done.
7. ~~**Add** Naga and pp-rs to acknowledgments.~~ Done (separate bullets).
8. ~~**Overhaul** `lpir/README.md` -- rationale, anti-corruption layer, examples, doc index.~~ Done.
9. ~~**Update** `docs/lpir/00-overview.md` -- Cranelift no longer "planned", pipeline shows
   interpreter, crate layout current.~~ Done.

## Conclusion

The **dependency graph is coherent**: firmware and `lp-engine` pull *
*`lpir-cranelift` + `lp-glsl-naga` + `lpir` + builtins** without pulling the filetest-only `glsl`
parser crate. Documentation now matches the post-refactor crate set across READMEs, the root README,
`CRATES.md`, `AGENTS.md`, the `docs/lpir/` spec, and acknowledgments.
