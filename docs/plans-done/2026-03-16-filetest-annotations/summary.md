# Filetest Annotation System — Implementation Summary

## Completed

Replaced the target/expect-fail system with a typed annotation system.

### Phases

1. **Core Types** — `target/mod.rs`, `target/display.rs`: `Backend`, `Isa`, `ExecMode`, `FloatMode`, `Target`, `TargetFilter`, `Annotation`, `AnnotationKind`, `Disposition`, `directive_disposition()`, `Target::name()`, `Target::from_name()`.

2. **Annotation Parser** — `parse/parse_annotation.rs`: parses `// @unimplemented()`, `// @broken(...)`, `// @ignore(...)` with optional key=value filters and reason.

3. **Parse Pipeline** — Updated `TestFile` (removed `target`, `is_test_run`; added `annotations`), `RunDirective` (removed `expect_fail`; added `annotations`). `parse_run.rs` no longer parses `[expect-fail]`; `legacy_expect_fail` kept for backward compatibility. `parse_source.rs` drops `// @...` lines from GLSL.

4. **Multi-Target Runner** — `test_run/compile.rs` for per-target compilation. Removed old `test_run/target.rs`. Runner iterates over targets; `run_summary`/`run_detail` use `directive_disposition`. `lib.rs`: `run_filetest_with_line_filter` takes `targets: &[&Target]`; `run()` takes `target_filter: Option<&'static Target>`.

5. **CLI** — `--target cranelift.q32` and `--target wasm.q32` in `lp-glsl-filetests-app`. `scripts/glsl-filetests.sh` passes through `--target`.

6. **Migration** — `scripts/migrate-filetest-annotations.py` migrated ~634 hand-written files: removed `// target`, converted `[expect-fail]` to `// @unimplemented()`, added file-level `// @unimplemented(backend=wasm)` where needed.

7. **Gen-App** — Updated vec generators (removed target line, added `@unimplemented(backend=wasm)` where applicable). Regenerated all `.gen.glsl` files. Fixed `find_filetests_dir()` paths in expand.rs/generator.rs.

8. **file_update.rs** — `add_annotation()`, `remove_annotation()`. `add_expect_fail_marker` and `remove_expect_fail_marker` delegate to these.

9. **Cleanup** — Removed `filetests/wasm/` directory. Fixed fn-equal generator: corrected D2 expected value for `in_expression` test (`equal(equal(a,b), equal(b,c))` = (false, true)). Fixed clippy warnings in lp-glsl-filetests.

### Validation

- `scripts/glsl-filetests.sh --target cranelift.q32`: 3927/3927 tests passed, 633 files.
- `cargo clippy -p lp-glsl-filetests -p lp-glsl-filetests-app -p lp-glsl-filetests-gen-app`: no warnings.
- `cargo +nightly fmt` applied.
