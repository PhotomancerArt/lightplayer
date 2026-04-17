# Summary — `lpir-inliner` stage ii (M1 `CompilerConfig` + `compile-opt`)

## Shipped

- **`lpir::compiler_config`**: `CompilerConfig`, `InlineConfig`, `InlineMode`, `ConfigError`, and `CompilerConfig::apply` for string keys (`inline.mode`, `inline.always_inline_single_site`, thresholds, optional budgets). `InlineMode`: `FromStr` / `Display` (`auto`, `always`, `never`). `no_std` + `alloc`.
- **Middle-end threading**: `config: CompilerConfig` on `NativeCompileOptions`, Cranelift `CompileOptions`, and `WasmOptions` (defaults via `CompilerConfig::default()`; options structs use `Clone` where `Copy` no longer applies).
- **Filetests**: `// compile-opt(key, value)` parsed in `parse_compile_opt.rs`; `TestFile::config_overrides`; duplicate keys rejected at parse time; `build_compiler_config` + merge before `compile_for_target`; GLSL output strips `compile-opt` lines; all backends in `filetest_lpvm` receive the merged config.

## Crates touched (main)

- `lp-shader/lpir` — `compiler_config.rs`, `lib.rs`
- `lp-shader/lpvm-native`, `lp-shader/lpvm-cranelift`, `lp-shader/lpvm-wasm`, `lp-shader/lpvm-emu` — options + clone/move fixes
- `lp-shader/lps-filetests` — parse, source strip, compile harness, `run_detail`
- `lp-core/lp-engine`, `lp-app/web-demo` — option struct literals

## Follow-ups

- **M4+**: Wire the inliner (and any other LPIR pass) to read `options.config.inline` (and friends).
- **Roadmap tagging**: Add `// compile-opt(inline.mode, never)` / `always` to the listed `filetests/function/*.glsl` when inliner behavior must be pinned.
- **`lp-server` / `fw-emu`**: Run `cargo check` if the full AGENTS matrix is required for a release; stage-ii phase 4 matrix covered shader pipeline crates + `fw-esp32` when run in CI.

## Validation (recorded at completion)

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-cranelift
cargo test -p lpvm-wasm
cargo test -p lps-filetests -- --test-threads=4
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
  --profile release-esp32 --features esp32c6,server
```

All of the above completed successfully before this summary was added.
