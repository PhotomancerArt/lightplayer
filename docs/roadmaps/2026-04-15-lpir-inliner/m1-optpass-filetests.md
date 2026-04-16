# M1 — Compiler Config + Filetest `compile-opt`

Add a **`// compile-opt(key, value)`** file directive to filetests for controlling
**LPIR optimization** options per file. Passes stay in the pipeline and consult
**`CompilerConfig`** (e.g. `inline.mode = never` skips inlining in the pass).

`CompilerConfig` is **not** part of the GLSL frontend (`lps-frontend`). It is a
**middle-end** concern: options for **LPIR-level** transforms (inline, future
passes) that run **after** lowering to LPIR and **before or during** lowering to
each backend. Backend-only knobs (native float mode / debug flags, Cranelift
memory strategy, WASM emit details) stay on each backend’s option struct and are
**layered** beside `CompilerConfig`, not merged into it.

## Design

### `compile-opt` directive

Single directive syntax for all **string-configurable** compiler (middle-end)
options. Conventionally placed **at the top of the file** (before `// run:` and
`// @…` lines):

```glsl
// compile-opt(inline.mode, never)
```

Parsed as a key-value pair: `key = "inline.mode"`, `value = "never"`.
The harness maps these to **`CompilerConfig`** before compilation.

This is **not** the same family as **`// @unimplemented(target)`** / etc.:
those are **target-scoped** and attach to the **next** `// run:`**.
**`compile-opt`** is **file-scoped** and applies to **how the whole module is
compiled** on every backend path that runs the LPIR pipeline.

### CompilerConfig

Top-level config struct that holds all **LPIR** optimization configs. Lives in
`lpir` (since passes live there). Must be `no_std`-compatible (`lpir` is
`#![no_std]` + `alloc`).

```rust
#[derive(Clone, Debug)]
pub struct CompilerConfig {
    pub inline: InlineConfig,
    // future: pub const_fold: ConstFoldConfig, etc.
}
```

Layering vs backends:

```
CompilerConfig          (LPIR passes: inline, const_fold config, …)  ← middle-end
  used alongside:
  NativeCompileOptions  (RV32 native: float_mode, debug_info, emu_trace, …)
  CompileOptions        (Cranelift: q32_options, memory_strategy, …)
  WasmOptions           (WASM: float_mode, …)
```

### InlineConfig

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineMode {
    /// Heuristic-based inlining (default).
    Auto,
    /// Inline everything unconditionally.
    Always,
    /// Skip all inlining.
    Never,
}

#[derive(Clone, Debug)]
pub struct InlineConfig {
    pub mode: InlineMode,
    pub always_inline_single_site: bool,
    pub small_func_threshold: usize,
    pub max_growth_budget: Option<usize>,
    pub module_op_budget: Option<usize>,
}
```

Defaults: `mode = Auto`, `always_inline_single_site = true`,
`small_func_threshold = 20`, budgets = `None`.

`InlineMode` implements `core::str::FromStr` so the type knows its own
names — no `std` dependency needed for parsing.

### Config application from key-value pairs

`CompilerConfig` has an `apply` method for mapping directive strings to
fields:

```rust
impl CompilerConfig {
    /// Apply a single key-value config override.
    /// Returns error on unknown key, invalid value, or duplicate key.
    pub fn apply(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        match key {
            "inline.mode" => {
                self.inline.mode = value.parse()
                    .map_err(|_| ConfigError::InvalidValue { key, value })?;
            }
            "inline.small_func_threshold" => {
                self.inline.small_func_threshold = value.parse()
                    .map_err(|_| ConfigError::InvalidValue { key, value })?;
            }
            "inline.max_growth_budget" => {
                self.inline.max_growth_budget = Some(value.parse()
                    .map_err(|_| ConfigError::InvalidValue { key, value })?);
            }
            "inline.module_op_budget" => {
                self.inline.module_op_budget = Some(value.parse()
                    .map_err(|_| ConfigError::InvalidValue { key, value })?);
            }
            _ => return Err(ConfigError::UnknownKey { key }),
        }
        Ok(())
    }
}
```

Unknown keys are errors (catches typos like `inlien.mode`). This is the single
place that knows the full key namespace — adding a new pass means adding match
arms here.

### Threading through compile options (everywhere)

**`CompilerConfig` must be available on every path that compiles LPIR** so
filetests and production agree regardless of target (JIT, RV32 Cranelift, RV32
native, WASM).

Add a `config: CompilerConfig` field to:

- **`NativeCompileOptions`** (`lpvm-native`)
- **`CompileOptions`** (`lpvm-cranelift`)
- **`WasmOptions`** (`lpvm-wasm`)

Example (native):

```rust
pub struct NativeCompileOptions {
    pub float_mode: lpir::FloatMode,
    pub debug_info: bool,
    pub emu_trace_instructions: bool,
    pub alloc_trace: bool,
    pub config: lpir::CompilerConfig,
}
```

These structs may drop **`Copy`** where they were **`Copy`** (`CompilerConfig`
is **`Clone`**). **`Default`** continues to use **`CompilerConfig::default()`**
for `config`.

Each pass reads the shared **`CompilerConfig`**. The const_fold and imm_fold
passes can remain unconditional for now (no config — cheap and always
beneficial). Add configs for them later if needed.

### Parsing (`compile-opt`)

Implement a **dedicated** parser (e.g. `parse_compile_opt_line`) — **not** an
`AnnotationKind` variant on `// @…(target)` lines.

```text
// compile-opt(inline.mode, never)
//           ^key          ^value
```

**Duplicate keys:** two `compile-opt` lines with the same key → error before
`CompilerConfig::apply`.

### Changes to TestFile

Add `config_overrides: Vec<(String, String)>` to `TestFile`. The compile path
merges these into **`CompilerConfig::default()`** and passes the result into
**each** backend’s options struct when building engines in the filetest
harness.

### Filetest harness flow

```
parse_compile_opt_line (or shared trim → try compile-opt first)
    │  // compile-opt(key, value) → push onto TestFile.config_overrides
    ▼
TestFile { config_overrides: Vec<(key, value)> }
    │
    ▼  (in compile_glsl, for every backend)
CompilerConfig::default()
    │  .apply(key, value) for each override (duplicate keys rejected earlier)
    ▼
CompileOptions { config, float_mode, .. }           // Jit / Rv32 c.flift
NativeCompileOptions { config, float_mode, .. }     // Rv32 native
WasmOptions { config, float_mode }                  // wasm
    │
    ▼
compile_module(ir, sig, options)
```

## Files to tag

Once the inliner is wired in (M4):

**Call-semantics tests** (keep real calls):

```glsl
// compile-opt(inline.mode, never)
```

- `filetests/function/call-simple.glsl`
- `filetests/function/call-multiple.glsl`
- `filetests/function/call-order.glsl`
- `filetests/function/call-return-value.glsl`

**Inliner correctness tests** (always inline, heuristic-proof):

```glsl
// compile-opt(inline.mode, always)
```

- New tests added in M4 specifically for inliner validation.

**Everything else:** No directive. Uses defaults (`Auto`).

## Changes by file

| File | Change |
|------|--------|
| `lpir/src/compiler_config.rs` (new) | `CompilerConfig`, `InlineConfig`, `InlineMode`, `ConfigError`, `apply()`. `InlineMode` impls `FromStr`. All `no_std`. |
| `lpir/src/lib.rs` | `pub mod compiler_config;` + re-exports |
| `lpvm-native/src/native_options.rs` | Add `config: CompilerConfig` |
| `lpvm-cranelift/src/compile_options.rs` | Add `config: CompilerConfig` (may drop `Copy` on `CompileOptions`) |
| `lpvm-wasm/src/options.rs` | Add `config: CompilerConfig` (may drop `Copy` on `WasmOptions`) |
| `lpvm-native/src/compile.rs` | Pass `config` to inline pass (M4). Optional: const_fold behind config later. |
| `lpvm-cranelift` / `lpvm-wasm` compile paths | Thread `config` through to wherever LPIR passes run (same as native when added) |
| `lps-filetests/src/parse/parse_compile_opt.rs` (new) or inline in `mod.rs` | Parse `// compile-opt(key, value)`; validate duplicate keys in `parse_test_file` |
| `lps-filetests/src/parse/mod.rs` | Recognize `compile-opt` before `@` annotations; collect into `TestFile` |
| `lps-filetests/src/parse/test_type.rs` | Add `config_overrides: Vec<(String, String)>` to `TestFile` |
| `lps-filetests/src/test_run/filetest_lpvm.rs` | Build `CompilerConfig`, thread into **all** `CompileOptions` / native / WASM builds |

Do **not** add `compile-opt` to `AnnotationKind` — keep `// @…` for
per-target / per-run annotations only.

## Validation

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lpvm-cranelift
cargo test -p lpvm-wasm
cargo test -p lps-filetests -- --test-threads=4
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
    --profile release-esp32 --features esp32c6,server
```

All existing filetests pass — no behavioral change until files use
`compile-opt` and the inliner is wired in (M4).
