# Phase 1: Migrate lp-engine to CraneliftEngine

## Scope

Update `lp-engine/src/gfx/cranelift.rs` to use `CraneliftEngine` trait API instead
of the legacy `jit()` function and `JitModule`.

This was intended to be part of M6 but was missed. Must be completed before we
can delete the old `jit()`/`JitModule` API.

## Code Changes

### `lp-core/lp-engine/src/gfx/cranelift.rs`

**Current:**

```rust
use lpvm_cranelift::{CompileOptions, DirectCall, FloatMode, JitModule, MemoryStrategy, jit};

pub struct CraneliftGraphics;

impl LpGraphics for CraneliftGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        let compile = CompileOptions {
            float_mode: FloatMode::Q32,
            q32_options: options.q32_options,
            memory_strategy: MemoryStrategy::Default,
            max_errors: options.max_errors,
        };
        let module = jit(source, &compile).map_err(|e| Error::Other {
            message: format!("{e}"),
        })?;
        let direct_call = module.direct_call("render");
        Ok(Box::new(CraneliftShader {
            _module: module,
            direct_call,
        }))
    }
}

struct CraneliftShader {
    _module: JitModule,
    direct_call: Option<DirectCall>,
}
```

**New:**

```rust
use lpvm_cranelift::{CraneliftEngine, CompileOptions, DirectCall, FloatMode, MemoryStrategy};

pub struct CraneliftGraphics {
    engine: CraneliftEngine,
}

impl CraneliftGraphics {
    pub fn new() -> Self {
        Self {
            engine: CraneliftEngine::new(CompileOptions::default()),
        }
    }
}

impl Default for CraneliftGraphics {
    fn default() -> Self {
        Self::new()
    }
}

impl LpGraphics for CraneliftGraphics {
    fn compile_shader(
        &self,
        source: &str,
        options: &ShaderCompileOptions,
    ) -> Result<Box<dyn LpShader>, Error> {
        // Build compile options for this specific compile
        let compile_opts = CompileOptions {
            float_mode: FloatMode::Q32,
            q32_options: options.q32_options,
            memory_strategy: MemoryStrategy::Default,
            max_errors: options.max_errors,
        };
        
        // Use the engine to compile (may need to add method to CraneliftEngine)
        let module = self.engine.compile_with_options(source, &compile_opts)
            .map_err(|e| Error::Other {
                message: format!("{e}"),
            })?;
        
        let direct_call = module.direct_call("render");
        Ok(Box::new(CraneliftShader {
            _module: module,
            direct_call,
        }))
    }
}

struct CraneliftShader {
    _module: CraneliftModule,  // Changed from JitModule
    direct_call: Option<DirectCall>,
}
```

### `lp-shader/lpvm-cranelift/src/lpvm_engine.rs` (possible addition)

If `CraneliftEngine::compile_with_options` doesn't exist, add it:

```rust
impl CraneliftEngine {
    /// Compile with specific options (not just engine defaults).
    pub fn compile_with_options(
        &self,
        source: &str,
        options: &CompileOptions,
    ) -> Result<CraneliftModule, CompilerError> {
        // Use existing compile logic
        crate::compile::jit(source, options)
            .map(|jit_module| CraneliftModule::from_jit_module(jit_module))
    }
}
```

Or we may need to check if `CraneliftEngine::compile()` already accepts options.

## Check CraneliftEngine API

Before making changes, verify:

- Does `CraneliftEngine::new()` take `CompileOptions`?
- Is there a per-compile method, or are options fixed at engine creation?
- If options are per-engine, we need to either:
  - Create a new engine per compile (heavy, not ideal)
  - Add `compile_with_options` method
  - Store `q32_options` in `CraneliftGraphics` and use single engine

## Code Organization Reminders

- Check existing `CraneliftEngine` implementation first
- Add `compile_with_options` if needed (at bottom of `lpvm_engine.rs`)
- Keep `CraneliftGraphics` simple - it's just a thin wrapper

## Validate

```bash
cargo check -p lp-engine --lib
cargo test -p lp-engine --tests
```

## Phase Notes

- Do NOT delete `jit()` or `JitModule` yet - this phase only migrates the consumer
- `CraneliftEngine` likely already exists; we may just need to use it differently
- If `CraneliftEngine` needs new methods, add them in this phase

