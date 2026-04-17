# Plan notes — `lpir-inliner` stage ii (M1 compiler config + filetest `compile-opt`)

## Scope of work

Implement **M1 — Compiler config + per-file opt overrides** from
`docs/roadmaps/2026-04-15-lpir-inliner/m1-optpass-filetests.md`, with the **syntax decision below** (replaces roadmap’s `@config` spelling).

- Add **`no_std` + `alloc`** `CompilerConfig` / `InlineConfig` / `InlineMode` / `ConfigError` in `lpir`, with `CompilerConfig::apply` for string key/value overrides (canonical key namespace for opt passes).
- Add **`config: CompilerConfig`** to **`NativeCompileOptions`**, Cranelift **`CompileOptions`**, and **`WasmOptions`**; passes read their slice of config (inline consumes when wired in later milestones).
- Extend **filetest parsing** with **`// compile-opt(key, value)`** (e.g. `// compile-opt(inline.mode, never)`), typically **at the top of the file**; store as **`TestFile::config_overrides`**, duplicate-key detection, merge into defaults before compilation in **`filetest_lpvm`** / compile path.
- **No intended behavior change** for existing tests: no new directive lines until we add them in a later milestone; inliner not wired until later roadmap work — defaults only.

Explicitly **out of scope** for this plan: M0 `CalleeRef` work (parallel track), actual inliner implementation, tagging individual `.glsl` files with `compile-opt` until a later milestone (e.g. M4) unless we add optional tagging in cleanup.

## Current state of the codebase (relevant to this scope)

- **Paths**: Shader stack lives under `lp-shader/` (`lpir`, `lpvm-native`, `lps-filetests`, etc.).
- **`lpir`**: `#![no_std]` + `alloc`; has `const_fold`, no `compiler_config` module yet. `FloatMode` already lives here and is reused by backends.
- **`NativeCompileOptions`** (`lp-shader/lpvm-native/src/native_options.rs`): `float_mode`, `debug_info`, `emu_trace_instructions`, `alloc_trace`; **`Copy`** + **`Default`**. Will likely **`Clone`** instead of **`Copy`** once it holds `CompilerConfig` (unless config is behind `Arc` — unlikely for tiny structs).
- **Filetest parse loop** (`lp-shader/lps-filetests/src/parse/mod.rs`): Lines matching `parse_annotation_line` are **target-scoped** (`@unimplemented(target)`, etc.) and accumulate in **`pending_annotations`**, then attach to the **next** `// run:`**.** File-level **`compile-opt`** must **not** use that pipeline.
- **New directive**: parse **`// compile-opt(...)`** in a dedicated path (comma-separated key/value inside parens, same logical shape as the old roadmap `@config` examples).
- **`Annotation` / `AnnotationKind`**: Keep **`AnnotationKind`** `Copy` for run annotations; **do not** add config here — use **`config_overrides`** on **`TestFile`**.
- **`CompiledShader::compile_glsl`** (`filetest_lpvm.rs`): builds **`FaCompileOptions`**, Cranelift **`CompileOptions`**, **`WasmOptions`** per target. **`CompilerConfig`** is **middle-end** (LPIR opts); it must thread into **all** of these so filetests and prod behave consistently on every backend (see updated **`m1-optpass-filetests.md`**).

## Questions (planning)

| # | Question | Status |
|---|----------|--------|
| 1 | Model config as `AnnotationKind::Config` vs **`TestFile::config_overrides`** + dedicated parse? | **Resolved** |
| 2 | Directive spelling for file-level overrides? | **Resolved** |
| 3 | Thread **`CompilerConfig`** only through native vs **all** backends? | **Resolved** |

### Suggested directions (for discussion)

_(Q1–Q2 resolved — see Answers.)_

## Answers (from chat)

### Q1 — Modeling

**Answer:** **`TestFile::config_overrides: Vec<(String, String)>`** plus a **dedicated** parse branch (e.g. `parse_compile_opt_line`), **not** `Annotation` / `AnnotationKind`. Do not push these lines into **`pending_annotations`**.

### Q2 — Syntax

**Answer:** Use **`// compile-opt(key, value)`** — file-level compiler / LPIR opt overrides, conventionally **at the top of the file**. Example: `// compile-opt(inline.mode, never)`.

**Rationale:** Keeps **`// @…(target)`** meaning “target-scoped, attaches to next **`// run:`**”; **`compile-opt`** reads as “how this file is compiled,” distinct from per-run annotations.

### Q3 — Where does `CompilerConfig` live conceptually, and who gets a field?

**Answer:** **`CompilerConfig` is middle-end (LPIR optimization pipeline)** — not **`lps-frontend`**, not backend-specific codegen toggles. **Thread `config: CompilerConfig` through every backend option struct** that compiles LPIR (`NativeCompileOptions`, **`CompileOptions`**, **`WasmOptions`**) so overrides apply everywhere; backend crates remain responsible for their **own** non-LPIR fields.

## Notes

- **Roadmap** `docs/roadmaps/2026-04-15-lpir-inliner/m1-optpass-filetests.md` is updated for **`compile-opt`**, middle-end framing, and **everywhere** threading.
- **Parallel work with M0 (stage i)**: M0 and M1 both touch **`lpvm-native`** and possibly **`lps-filetests`**; **`lpir`** gains new files in both. Expect occasional rebase conflicts; **M1 does not depend on enum `CalleeRef`** for `CompilerConfig` itself. Merge order: land M0 first if both touch the same lines, or coordinate.
- **`NativeCompileOptions` non-`Copy`**: All struct literals and `#[derive(Copy)]` call sites need review after adding **`CompilerConfig`**.
