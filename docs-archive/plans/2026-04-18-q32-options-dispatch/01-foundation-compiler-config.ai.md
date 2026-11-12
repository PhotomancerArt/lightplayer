# Phase 1 — Foundation: `CompilerConfig` promotion + engine glue

## Scope of phase

Promote `Q32Options` (`lps-q32::q32_options::Q32Options`) into
`lpir::CompilerConfig` as a new `q32` field, so it gets the same plumbing
the existing `inline` field gets. Wire `lp-engine` glue to set this field
instead of dropping `options.q32_options` (as it does today at
`lp-engine/src/gfx/native_jit.rs:89`).

This phase is purely plumbing: no new dispatch logic, no behavior change.
After this phase, defaults still produce identical output across the board.

**Out of scope:**

- Implementing dispatch in any backend (phases 3 and 4).
- The new `__lp_lpir_fdiv_recip_q32` helper (phase 2).
- Filetests (phase 5).
- Removing the existing top-level `q32_options` fields on
  `ShaderCompileOptions` (lp-engine) or cranelift's `CompileOptions` —
  those stay for API stability, just made consistent with `config.q32`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix them.
- Do **not** disable, skip, or weaken existing tests to make the build
  pass.
- If something blocks completion (ambiguity, unexpected design issue), stop
  and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from
  the phase plan.

## Implementation Details

### Step 1: Add `lpir → lps-q32` dependency

File: `lp-shader/lpir/Cargo.toml`

Add to `[dependencies]`:

```toml
lps-q32 = { path = "../lps-q32" }
```

Verify `lps-q32`'s own `Cargo.toml` does NOT depend on `lpir` (it currently
only depends on `libm` — confirm). Both crates are no-std, so no feature
flag work should be needed.

### Step 2: Add FromStr impls for the three Q32 mode enums

File: `lp-shader/lps-q32/src/q32_options.rs`

`CompilerConfig::apply` will parse string values like `"saturating"`,
`"wrapping"`, `"reciprocal"`. Add `FromStr` impls so we don't have to
inline the matching everywhere. Pattern: lowercase only, error on unknown.

```rust
impl core::str::FromStr for AddSubMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "saturating" => Ok(AddSubMode::Saturating),
            "wrapping"   => Ok(AddSubMode::Wrapping),
            _ => Err(()),
        }
    }
}

impl core::str::FromStr for MulMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "saturating" => Ok(MulMode::Saturating),
            "wrapping"   => Ok(MulMode::Wrapping),
            _ => Err(()),
        }
    }
}

impl core::str::FromStr for DivMode {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "saturating" => Ok(DivMode::Saturating),
            "reciprocal" => Ok(DivMode::Reciprocal),
            _ => Err(()),
        }
    }
}
```

Add unit tests in the same file under the existing pattern (currently the
file has no tests; add `#[cfg(test)] mod tests` at the bottom):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_sub_from_str() {
        assert_eq!("saturating".parse::<AddSubMode>(), Ok(AddSubMode::Saturating));
        assert_eq!("wrapping".parse::<AddSubMode>(), Ok(AddSubMode::Wrapping));
        assert!("bogus".parse::<AddSubMode>().is_err());
    }

    #[test]
    fn mul_from_str() {
        assert_eq!("saturating".parse::<MulMode>(), Ok(MulMode::Saturating));
        assert_eq!("wrapping".parse::<MulMode>(), Ok(MulMode::Wrapping));
        assert!("reciprocal".parse::<MulMode>().is_err());
    }

    #[test]
    fn div_from_str() {
        assert_eq!("saturating".parse::<DivMode>(), Ok(DivMode::Saturating));
        assert_eq!("reciprocal".parse::<DivMode>(), Ok(DivMode::Reciprocal));
        assert!("wrapping".parse::<DivMode>().is_err());
    }
}
```

### Step 3: Add `q32` field to `CompilerConfig`

File: `lp-shader/lpir/src/compiler_config.rs`

Current shape:

```rust
pub struct CompilerConfig {
    pub inline: InlineConfig,
}
```

Change to:

```rust
pub struct CompilerConfig {
    pub inline: InlineConfig,
    pub q32: lps_q32::q32_options::Q32Options,
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self {
            inline: InlineConfig::default(),
            q32: lps_q32::q32_options::Q32Options::default(),
        }
    }
}
```

Add `apply` arms for the three keys:

```rust
impl CompilerConfig {
    pub fn apply(&mut self, key: &str, value: &str) -> Result<(), ConfigError> {
        match key.trim() {
            // ... existing inline.* arms unchanged ...

            "q32.add_sub" => {
                self.q32.add_sub = value.trim().parse().map_err(|_| invalid(key, value))?;
            }
            "q32.mul" => {
                self.q32.mul = value.trim().parse().map_err(|_| invalid(key, value))?;
            }
            "q32.div" => {
                self.q32.div = value.trim().parse().map_err(|_| invalid(key, value))?;
            }
            _ => {
                return Err(ConfigError::UnknownKey {
                    key: String::from(key),
                });
            }
        }
        Ok(())
    }
}
```

Add unit tests next to the existing `apply_*` tests:

```rust
#[test]
fn apply_q32_add_sub() {
    let mut c = CompilerConfig::default();
    assert_eq!(c.q32.add_sub, lps_q32::q32_options::AddSubMode::Saturating);
    c.apply("q32.add_sub", "wrapping").unwrap();
    assert_eq!(c.q32.add_sub, lps_q32::q32_options::AddSubMode::Wrapping);
    c.apply("q32.add_sub", "saturating").unwrap();
    assert_eq!(c.q32.add_sub, lps_q32::q32_options::AddSubMode::Saturating);
}

#[test]
fn apply_q32_mul() {
    let mut c = CompilerConfig::default();
    c.apply("q32.mul", "wrapping").unwrap();
    assert_eq!(c.q32.mul, lps_q32::q32_options::MulMode::Wrapping);
}

#[test]
fn apply_q32_div() {
    let mut c = CompilerConfig::default();
    c.apply("q32.div", "reciprocal").unwrap();
    assert_eq!(c.q32.div, lps_q32::q32_options::DivMode::Reciprocal);
}

#[test]
fn apply_q32_invalid_value_errors() {
    let mut c = CompilerConfig::default();
    assert!(c.apply("q32.add_sub", "bogus").is_err());
    assert!(c.apply("q32.mul", "reciprocal").is_err());  // wrong enum
    assert!(c.apply("q32.div", "wrapping").is_err());    // wrong enum
}
```

### Step 4: Update `lp-engine` glue to stop dropping `q32_options`

There are three glue sites. All three currently take `&ShaderCompileOptions`
(which has `q32_options: lps_q32::q32_options::Q32Options`).

#### `lp-core/lp-engine/src/gfx/native_jit.rs`

Currently at line ~89:

```rust
let _ = (options.max_errors, options.q32_options);
```

This explicitly drops both. The `max_errors` drop is fine (front-end max
errors aren't plumbed yet). Replace with: build the `NativeCompileOptions`
to actually carry `q32_options` via `config.q32`.

Find the `NativeCompileOptions { ... }` literal a few lines above. It
currently looks like:

```rust
let engine = NativeJitEngine::new(
    Arc::clone(&self.builtin_table),
    NativeCompileOptions {
        float_mode: lpir::FloatMode::Q32,
        debug_info: false,
        emu_trace_instructions: false,
        alloc_trace: false,
        ..Default::default()
    },
);
```

Update to:

```rust
let engine = NativeJitEngine::new(
    Arc::clone(&self.builtin_table),
    NativeCompileOptions {
        float_mode: lpir::FloatMode::Q32,
        debug_info: false,
        emu_trace_instructions: false,
        alloc_trace: false,
        config: lpir::CompilerConfig {
            q32: options.q32_options,
            ..Default::default()
        },
        ..Default::default()
    },
);

let _ = options.max_errors; // TODO: thread max_errors when front-end accepts it
```

Drop the `let _ = options.q32_options` line entirely (it's now consumed).

#### `lp-core/lp-engine/src/gfx/native_object.rs`

Same pattern — find the `NativeCompileOptions { ... }` literal and add the
`config: lpir::CompilerConfig { q32: options.q32_options, ..Default::default() }`
field. Drop any `let _ = options.q32_options` if it exists.

#### `lp-core/lp-engine/src/gfx/cranelift.rs`

Currently at line ~49:

```rust
q32_options: options.q32_options,
```

This stays — cranelift's `CompileOptions` keeps the top-level field for API
stability. **Also** set the new `config.q32` field for consistency:

```rust
let cranelift_opts = lpvm_cranelift::CompileOptions {
    float_mode: ...,
    q32_options: options.q32_options,             // existing top-level
    config: lpir::CompilerConfig {                // new — keep in sync
        q32: options.q32_options,
        ..Default::default()
    },
    ..Default::default()
};
```

Verify cranelift's `CompileOptions` already has a `config: CompilerConfig`
field (it should — see `lp-shader/lpvm-cranelift/src/compile_options.rs`).
If `config` field doesn't exist, this means cranelift doesn't carry
`CompilerConfig` yet — in that case, just leave the top-level `q32_options`
assignment unchanged and add a TODO comment noting the inconsistency
(should not be the case based on the design notes audit, but check).

### Step 5: Add TODO note to cranelift compile_options

File: `lp-shader/lpvm-cranelift/src/compile_options.rs`

Add a doc comment to the `q32_options` field explaining it's vestigial
post-2026-04-18 (codegen ignores it; `config.q32` is the actual source of
truth, but cranelift's emit/scalar.rs doesn't dispatch on either):

```rust
pub struct CompileOptions {
    pub float_mode: FloatMode,
    /// Vestigial — cranelift codegen does not dispatch on Q32 mode.
    /// Use `config.q32` for the real value once cranelift gains dispatch
    /// (cranelift JIT is currently deprecated; WASM and native are the
    /// supported paths). Engine glue keeps both fields in sync.
    pub q32_options: Q32Options,
    pub memory_strategy: MemoryStrategy,
    pub max_errors: Option<usize>,
    pub emu_trace_instructions: bool,
    pub config: CompilerConfig,
}
```

## Validate

From workspace root:

```bash
cargo check -p lps-q32 -p lpir
cargo test -p lps-q32 -p lpir
cargo check -p lp-engine
cargo build --workspace
```

If any test fails or the workspace doesn't build cleanly, fix it before
reporting done. Defaults must produce identical output across the board
(no behavior change in this phase).

## Definition of done

- `lps-q32` exposes `FromStr` impls for `AddSubMode`, `MulMode`, `DivMode`
  with passing tests.
- `CompilerConfig` has a `q32: Q32Options` field, defaults to
  `Q32Options::default()`.
- `CompilerConfig::apply` accepts `q32.add_sub`, `q32.mul`, `q32.div` keys
  with passing tests.
- `lp-engine`'s three gfx glue sites build `NativeCompileOptions` /
  `CompileOptions` with `config.q32 = options.q32_options`. The
  `let _ = options.q32_options` line in `native_jit.rs` is gone.
- Cranelift's `CompileOptions::q32_options` field has a clarifying
  doc comment noting it's vestigial.
- `cargo build --workspace` and the targeted tests pass.
- No new warnings introduced.
