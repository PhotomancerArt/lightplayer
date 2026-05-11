# M1 — Compiler Config + Filetest `@config` Annotation

Add a `@config(key, value)` annotation to filetests for controlling
compiler options per file. All optimizations are always in the pipeline —
they disable themselves via their own config (e.g. `inline.mode = never`).

## Design

### `@config` annotation

Single annotation syntax for all compiler options:

```glsl
// @config(inline.mode, never)
```

Parsed as a key-value pair: `key = "inline.mode"`, `value = "never"`.
The harness maps these to the appropriate config structs before compilation.

### CompilerConfig

Top-level config struct that holds all optimization configs. Lives in
`lpir` (since passes live there). Must be `no_std`-compatible (`lpir` is
`#![no_std]` + `alloc`).

```rust
#[derive(Clone, Debug)]
pub struct CompilerConfig {
    pub inline: InlineConfig,
    // future: pub const_fold: ConstFoldConfig, etc.
}
```

`CompilerConfig` is about LPIR-level optimization passes. It's separate
from backend-specific options (`NativeCompileOptions` has float_mode,
debug_info, etc.). They're layered, not merged:

```
CompilerConfig             (LPIR-level: inline, const_fold, future passes)
  └─ NativeCompileOptions  (backend-level: float_mode, debug_info, emu_trace)
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

`CompilerConfig` has an `apply` method for mapping annotation strings to
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

Unknown keys are parse errors (catches typos like `inlien.mode`). This
is the single place that knows the full key namespace — adding a new pass
means adding match arms here.

### Threading through compile options

`NativeCompileOptions` gets a `config: CompilerConfig` field:

```rust
pub struct NativeCompileOptions {
    pub float_mode: lpir::FloatMode,
    pub debug_info: bool,
    pub emu_trace_instructions: bool,
    pub alloc_trace: bool,
    pub config: lpir::CompilerConfig,
}
```

Each pass checks its own config. The const_fold and imm_fold passes
can remain unconditional for now (no config needed — they're cheap and
always beneficial). Add configs for them later if needed.

### Annotation parsing

Extend `parse_annotation.rs` to handle `@config`:

```rust
// @config(inline.mode, never)
//         ^key          ^value
```

New annotation kind: `AnnotationKind::Config { key: String, value: String }`.

`@config` is **not target-scoped** (unlike `@unimplemented(target)`).
It applies to the LPIR-level source, not a specific backend. If
target-specific config is ever needed, a third parameter can be added
later.

### Duplicate key handling

If a file has two `@config` lines with the same key, that's an error:

```glsl
// @config(inline.mode, never)
// @config(inline.mode, always)   // ERROR: duplicate key 'inline.mode'
```

The harness tracks seen keys and rejects duplicates before calling
`CompilerConfig::apply`.

### Changes to TestFile

Add `config_overrides: Vec<(String, String)>` to `TestFile`. The compile
path merges these into the default `CompilerConfig` before compilation.

### Filetest harness flow

```
parse_annotation_line
    │  @config(key, value) → AnnotationKind::Config { key, value }
    ▼
TestFile { config_overrides: Vec<(key, value)> }
    │
    ▼  (in compile_glsl)
CompilerConfig::default()
    │  .apply(key, value) for each override
    ▼
NativeCompileOptions { config, float_mode, .. }
    │
    ▼
compile_module(ir, sig, options)
```

## Files to tag

Once the inliner is wired in (M4):

**Call-semantics tests** (keep real calls):
```glsl
// @config(inline.mode, never)
```
- `filetests/function/call-simple.glsl`
- `filetests/function/call-multiple.glsl`
- `filetests/function/call-order.glsl`
- `filetests/function/call-return-value.glsl`

**Inliner correctness tests** (always inline, heuristic-proof):
```glsl
// @config(inline.mode, always)
```
- New tests added in M4 specifically for inliner validation.

**Everything else:** No annotation. Uses defaults (`Auto`).

## Changes by file

| File | Change |
|------|--------|
| `lpir/src/compiler_config.rs` (new) | `CompilerConfig`, `InlineConfig`, `InlineMode`, `ConfigError`, `apply()` method. `InlineMode` impls `FromStr`. All `no_std`. |
| `lpir/src/lib.rs` | `pub mod compiler_config;` + re-exports |
| `lpvm-native/src/native_options.rs` | Add `config: CompilerConfig` field to `NativeCompileOptions` |
| `lpvm-native/src/compile.rs` | Pass config to inline pass (M4). Guard const_fold/imm_fold behind config checks if configs are added for them. |
| `lps-filetests/src/parse/parse_annotation.rs` | Add `Config` annotation kind, parse `@config(key, value)` |
| `lps-filetests/src/parse/mod.rs` | Collect config annotations into `TestFile`, check for duplicate keys |
| `lps-filetests/src/parse/test_type.rs` | Add `config_overrides: Vec<(String, String)>` to `TestFile` |
| `lps-filetests/src/test_run/filetest_lpvm.rs` | Build `CompilerConfig` from overrides, thread into compile options |
| `lps-filetests/src/targets/mod.rs` | Add `Config` to `AnnotationKind` |

## Validation

```bash
cargo test -p lpir
cargo test -p lpvm-native
cargo test -p lps-filetests -- --test-threads=4
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
    --profile release-esp32 --features esp32c6,server
```

All existing filetests pass — no behavioral change since no files have
`@config` annotations yet, and the inliner isn't wired in until M4.
