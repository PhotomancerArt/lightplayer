# Stage VII summary: delete old compiler chain

## Removed crates (paths)

- `lp-glsl/lp-glsl-cranelift` — legacy TypedShader → Cranelift
- `lp-glsl/lp-glsl-jit-util` — JIT calling convention helpers for that stack
- `lp-glsl/lp-glsl-frontend` — `glsl`-crate semantic frontend used only by the above
- `lp-glsl/esp32-glsl-jit` — pre-`lp-fw` ESP32 test app
- `lp-glsl/lp-glsl-q32-metrics-app` — Q32 metrics tool wired to the old compiler

## Migrations

- **`lp-glsl-builtins-gen-app`:** Local `lpfx/types.rs` + `signature_parse.rs` replace `lp-glsl-frontend` types; `grouping.rs` replaces the old `generate.rs` LPFX table output. Stopped generating `registry.rs`, `mapping.rs`, and `lpfx_fns.rs` (deleted targets).
- **`lp-glsl-filetests`:** `TranslationUnit::parse` via `glsl::parser::Parse` instead of `CompilationPipeline::parse`.
- **Workspace:** Dropped members/default-members and `profile.release.package.esp32-glsl-jit`; removed `Dockerfile.rv32-jit`.

## Build / IDE / scripts

- **`justfile`:** Dropped old package lists; `build-rv32` no longer builds `esp32-glsl-jit`; removed `build-rv32-jit-test` / `clippy-rv32-jit-test`.
- **`scripts/lp-build.sh`:** Second step is `cargo check -p fw-esp32 …` from repo root.
- **`scripts/q32-metrics.sh`:** Stub that exits 1 (metrics app removed).
- **`.idea/lp2025.iml`:** Removed source entries for deleted crates.

## Docs

- **`README.md`**, **`lp-glsl/README.md`**, **`AGENTS.md`**, **`.cursor/rules/no-std-compile-path.mdc`:** Describe only the naga → LPIR → `lpir-cranelift` path.

## Tests

- **`lpfx_builtins_memory`:** Fixed builtins WASM export name to `__lp_lpfx_saturate_vec3_q32`. Shader integration test remains **`#[ignore]`** — WASM import ABI mismatch for vec3 LPFX; see `docs/roadmaps/2026-03-25-lpir-features/`.

## Follow-up

- Align LPIR→WASM vec3 multi-return with builtins result-pointer ABI (roadmap above), then un-ignore the shader test.
