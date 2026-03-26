# Phase 3: CompileOptions expansion

## Scope

Add `Q32Options`, `MemoryStrategy`, and `max_errors` to `CompileOptions`.
Update all call sites and tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. New file: `q32_options.rs`

```rust
//! Q32 arithmetic mode selection for code generation.

/// Per-shader Q32 arithmetic options controlling builtin selection.
///
/// These are compiler-internal types. `lp-engine` maps `lp_model::GlslOpts`
/// to these at the call site (Stage VI-B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Q32Options {
    pub add_sub: AddSubMode,
    pub mul: MulMode,
    pub div: DivMode,
}

impl Default for Q32Options {
    fn default() -> Self {
        Self {
            add_sub: AddSubMode::default(),
            mul: MulMode::default(),
            div: DivMode::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AddSubMode {
    #[default]
    Saturating,
    Wrapping,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MulMode {
    #[default]
    Saturating,
    Wrapping,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DivMode {
    #[default]
    Saturating,
    Reciprocal,
}
```

### 2. New enum in `compile_options.rs`: `MemoryStrategy`

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MemoryStrategy {
    #[default]
    Default,
    LowMemory,
}
```

### 3. Expand `CompileOptions`

```rust
use crate::q32_options::Q32Options;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileOptions {
    pub float_mode: FloatMode,
    pub q32_options: Q32Options,
    pub memory_strategy: MemoryStrategy,
    pub max_errors: Option<usize>,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
            q32_options: Q32Options::default(),
            memory_strategy: MemoryStrategy::default(),
            max_errors: None,
        }
    }
}
```

### 4. Update `lib.rs` exports

```rust
mod q32_options;

pub use compile_options::{CompileOptions, MemoryStrategy};
pub use q32_options::{AddSubMode, DivMode, MulMode, Q32Options};
```

### 5. Update call sites

All places that construct `CompileOptions` need the new fields. Since
`Default` covers the common case, most can use struct update syntax:

```rust
CompileOptions {
    float_mode: FloatMode::Q32,
    ..Default::default()
}
```

Or just `CompileOptions::default()` where Q32 + Default strategy is wanted.

Search for `CompileOptions {` across:
- `lpir-cranelift/src/lib.rs` (tests)
- `lp-glsl-filetests/` (compile dispatch)
- Any other callers

Note: `Q32Options` is plumbed and stored but the emitter does **not** yet
branch on mode variants — it unconditionally emits saturating builtins.
Wiring wrapping/reciprocal builtins is a follow-up when those builtins exist.
Similarly, `max_errors` is stored but not enforced in the compilation loop
yet — enforcement can be added when the error accumulation path exists.

### 6. Tests

```rust
#[test]
fn compile_options_default() {
    let opts = CompileOptions::default();
    assert_eq!(opts.float_mode, FloatMode::Q32);
    assert_eq!(opts.q32_options, Q32Options::default());
    assert_eq!(opts.memory_strategy, MemoryStrategy::Default);
    assert_eq!(opts.max_errors, None);
}

#[test]
fn q32_options_default_is_saturating() {
    let q = Q32Options::default();
    assert_eq!(q.add_sub, AddSubMode::Saturating);
    assert_eq!(q.mul, MulMode::Saturating);
    assert_eq!(q.div, DivMode::Saturating);
}
```

## Validate

```bash
cargo test -p lpir-cranelift
cargo test -p lpir-cranelift --features riscv32-emu
cargo check --target riscv32imac-unknown-none-elf -p lpir-cranelift --no-default-features

# Filetests (may construct CompileOptions)
just glsl-filetests
```
