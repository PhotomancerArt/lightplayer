# Phase 5: Firmware Integration

## Scope

Integrate the JIT engine into `fw-emu` and `fw-esp32` firmwares.

## Implementation Details

### 1. Update `fw-esp32/src/main.rs`

Add JIT engine initialization at startup:

```rust
// At top of main.rs, add imports
#[cfg(target_arch = "riscv32")]
use lpvm_native::rt_jit::{BuiltinTable, NativeJitEngine};
#[cfg(target_arch = "riscv32")]
use alloc::sync::Arc;

// In main() or init function:
#[cfg(target_arch = "riscv32")]
fn init_jit_engine() -> NativeJitEngine {
    // Ensure builtins are not dead-code-eliminated
    lps_builtins::ensure_builtins_referenced();
    
    // Populate builtin table
    let mut table = BuiltinTable::new();
    table.populate();
    
    // Verify table has entries
    assert!(!table.is_empty(), "BuiltinTable empty - builtins not linked?");
    
    // Create engine with populated table
    let table = Arc::new(table);
    let options = lpvm_native::NativeCompileOptions {
        float_mode: lpir::FloatMode::Q32,
        alloc_trace: false,
        emu_trace_instructions: false,
    };
    
    NativeJitEngine::new(table, options)
}

// Store engine in your app state or pass to shader runtime
```

### 2. Update `fw-emu/src/main.rs`

Same pattern as fw-esp32:

```rust
#[cfg(target_arch = "riscv32")]
use lpvm_native::rt_jit::{BuiltinTable, NativeJitEngine};

fn main() {
    // ... existing init code ...
    
    #[cfg(target_arch = "riscv32")]
    let jit_engine = init_jit_engine();
    
    // Pass engine to shader runtime or store in app state
}

#[cfg(target_arch = "riscv32")]
fn init_jit_engine() -> NativeJitEngine {
    lps_builtins::ensure_builtins_referenced();
    
    let mut table = BuiltinTable::new();
    table.populate();
    
    let table = Arc::new(table);
    let options = lpvm_native::NativeCompileOptions::default();
    
    NativeJitEngine::new(table, options)
}
```

### 3. Update `fw-esp32/Cargo.toml` and `fw-emu/Cargo.toml`

Ensure dependencies are correct:

```toml
[dependencies]
# ... existing deps ...
lpvm-native = { path = "../../lp-shader/lpvm-native", default-features = false }
```

No explicit feature needed - `rt_jit` is auto-included on `riscv32` target.

### 4. Integration with shader runtime

In your shader runtime code (where you compile shaders):

```rust
// Use the JIT engine instead of Cranelift engine
pub fn compile_shader(&self, source: &str) -> Result<CompiledShader, Error> {
    // Parse GLSL to LPIR
    let ir = lps_frontend::parse(source)?;
    let meta = lps_frontend::extract_meta(&ir)?;
    
    // Compile using JIT engine
    let module = self.jit_engine.compile(&ir, &meta)?;
    
    Ok(CompiledShader { module })
}
```

## Verify Firmware Builds

```bash
# ESP32 firmware
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# Emulator firmware
cargo check -p fw-emu --target riscv32imac-unknown-none-elf

# Full release build (verify it compiles and links)
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server 2>&1 | head -50
```

## Expected Binary Size Impact

Positive changes:
- Remove `object` crate dependency (~50KB saved)
- Remove ELF linking code (~10KB saved)

Negative changes:
- Add `rt_jit` module (~5KB added)

Net: ~55KB flash savings expected.

## Next Phase

Once firmware integration is done, proceed to Phase 6: Testing and cleanup.
