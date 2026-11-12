# M1 Plan - Resumable Scaffold and Top-Level Index

## Goal

Create the smallest durable scaffold for `lp-shader/lps-glsl`: a `no_std + alloc` crate, a
resumable compile job API, token/span/diagnostic foundations, top-level indexing for current
examples, and a side-by-side filetest target named `rv32lpn.q32`.

M1 does not lower function bodies to LPIR. It proves the rails.

## Scope

In scope:

- Add `lp-shader/lps-glsl` to the workspace and default members.
- Expose `lps_glsl::CompileJob` plus a synchronous wrapper.
- Add `Span`, `SourceMap`, diagnostic shell types, token kinds, lexer, and token tape.
- Add shallow top-level index scanning for:
  - `layout(binding = N) uniform ...;`
  - global `const ...;`
  - function signatures
  - function body spans
- Add `rv32lpn.q32` to filetest target parsing/display.
- Route `rv32lpn.q32` through `lps-glsl` in the filetest compile seam, initially returning a clear
  "body lowering not implemented yet" error after successful indexing.
- Add tests that every current `examples/**/*.glsl` file lexes and indexes.

Out of scope:

- HIR design beyond placeholder API naming.
- LPIR lowering.
- Semantic type checking.
- Embedded scheduler integration.
- Making `lps-glsl` the production default.

## File Structure

Expected new crate shape:

```text
lp-shader/lps-glsl/
  Cargo.toml
  src/
    lib.rs
    compile.rs
    diagnostic.rs
    index.rs
    job.rs
    lexer.rs
    source.rs
    token.rs
```

Expected existing files to touch:

```text
Cargo.toml
lp-shader/lps-filetests/Cargo.toml
lp-shader/lps-filetests/src/targets/mod.rs
lp-shader/lps-filetests/src/targets/display.rs
lp-shader/lps-filetests/src/test_run/filetest_lpvm.rs
```

## Public API Shape

Use short names inside the crate:

```rust
pub struct CompileJob<'src> { ... }
pub struct CompileOptions { ... }
pub struct CompileBudget { ... }
pub enum CompileStepResult { Pending, Finished(CompileOutput), Failed(Diagnostic) }
pub struct CompileOutput { ... }

pub fn compile(source: &str, options: &CompileOptions) -> Result<CompileOutput, Diagnostic>;
pub fn index_source(source: &str) -> Result<TopLevelIndex, Diagnostic>;
```

`compile(...)` should loop over `CompileJob::step(...)`. For M1, the job can stop after top-level
indexing and return a planned unsupported diagnostic for full compilation. The separate
`index_source(...)` helper gives tests and filetest plumbing a useful M1 validation surface.

Avoid `LightCompileJob`. If an engine-facing name later needs a product prefix, use an alias or
wrapper such as `LpCompileJob`.

## Filetest Integration

Add an internal frontend distinction to the target model while exposing it as a normal target name:

```text
rv32n.q32    Naga frontend + lpvm-native backend
rv32lpn.q32  lps-glsl frontend + lpvm-native backend
```

Implementation suggestion:

- Add `Frontend { Naga, Lp }` or equivalent to `Target`.
- Existing targets use `Frontend::Naga`.
- Add one new `ALL_TARGETS` entry for `rv32lpn.q32`.
- Update display/parsing so `rv32lpn` shorthand works.
- In `CompiledShader::compile_glsl`, branch before backend compilation:
  - Naga: current `lower_glsl(...)`.
  - Lp: call `lps_glsl::compile(...)`.

For M1, `rv32lpn.q32` is allowed to compile-fail with a clear diagnostic after indexing. The point
is that the target exists, can be selected, and flows through the intended seam.

## Lexer and Index Requirements

Lexer must support enough tokenization for all current examples:

- comments and whitespace skipping
- identifiers and keywords
- integer, unsigned integer, and float literals, including leading-dot floats like `.3`
- punctuation/operators used by examples
- source spans for every token

Top-level scanner must be shallow and brace-aware:

- record function body spans without parsing bodies
- skip function bodies when looking for later declarations
- parse function return type, name, and parameter type/name pairs well enough for examples
- record uniforms and global constants with spans
- produce precise first-error diagnostics for malformed top-level syntax

## Tests

Add crate unit tests or integration tests that cover:

- tokenization of representative example snippets
- source map line/column lookup
- top-level indexing of each current example shader:
  - `examples/fast/shader.glsl`
  - `examples/basic2/shader.glsl`
  - `examples/basic/shader.glsl`
  - `examples/noise.fx/main.glsl`
  - `examples/perf/baseline/shader.glsl`
  - `examples/perf/fastmath/shader.glsl`
  - `examples/rocaille/shader.glsl`
- `rv32lpn.q32` parses as a valid target and formats back to the same name
- `rv32n.q32` behavior remains unchanged

Do not add filetest annotations yet.

## Validation

Run:

```bash
cargo test -p lps-glsl
cargo test -p lps-filetests targets
cargo check -p lps-filetests
cargo check -p lps-filetests-app
```

Optional smoke check once the target exists:

```bash
cargo run -p lps-filetests-app -- test --target rv32n.q32,rv32lpn.q32 examples/fast/shader.glsl --concise
```

The smoke check may report `rv32lpn.q32` compile-fail in M1, but it should not fail because the
target name is unknown or because the Naga/native path regressed.

## Completion Criteria

- `lp-shader/lps-glsl` builds on host.
- All example shaders lex and top-level-index in tests.
- `rv32lpn.q32` is a known filetest target.
- The filetest compile seam routes `rv32lpn.q32` to `lps-glsl`.
- Existing `rv32n.q32` remains on the Naga frontend path.
- No production compile path changes unless explicitly selecting `rv32lpn.q32`.
