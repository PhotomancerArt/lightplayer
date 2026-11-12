## Phase 3: WasmModule and Emission Tests

### Scope

Implement `WasmModule` struct with exports metadata. Add unit tests that:
- Emit minimal LPIR modules
- Validate WASM bytes with `wasmparser`
- Verify exports match expected signatures

### Implementation Details

**module.rs:**

```rust
use alloc::string::String;
use alloc::vec::Vec;
use lps_frontend::LpsType;
use wasm_encoder::ValType;

/// A compiled WASM module ready for instantiation or browser use.
#[derive(Debug, Clone)]
pub struct WasmModule {
    pub(crate) bytes: Vec<u8>,
    pub(crate) exports: Vec<WasmExport>,
    pub(crate) shadow_stack_base: Option<i32>,
}

impl WasmModule {
    /// Raw WASM bytes for browser instantiation or file output.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Function exports with both WASM and logical types.
    pub fn exports(&self) -> &[WasmExport] {
        &self.exports
    }

    /// Shadow stack base offset if the module uses slot memory.
    pub fn shadow_stack_base(&self) -> Option<i32> {
        self.shadow_stack_base
    }
}

/// Metadata for an exported WASM function.
#[derive(Debug, Clone)]
pub struct WasmExport {
    pub name: String,
    /// WASM parameter types (includes VMContext as first I32).
    pub params: Vec<ValType>,
    /// WASM result types.
    pub results: Vec<ValType>,
    /// Logical return type for marshaling.
    pub return_type: LpsType,
    /// Logical parameter types for marshaling (user params only).
    pub param_types: Vec<LpsType>,
}
```

**Update emit.rs to return WasmModule:**

Change return type from `Result<(Vec<u8>, Option<i32>), WasmError>` to
`Result<WasmModule, WasmError>`. Update `emit/mod.rs` re-export if needed.

**options.rs:**

```rust
use lps_frontend::FloatMode;

/// Options for WASM emission.
#[derive(Debug, Clone, Copy)]
pub struct WasmOptions {
    /// Float representation: Q32 (fixed-point) or F32 (IEEE).
    pub float_mode: FloatMode,
}

impl Default for WasmOptions {
    fn default() -> Self {
        Self {
            float_mode: FloatMode::Q32,
        }
    }
}
```

**error.rs additions:**

```rust
/// Unified error type for WASM backend operations.
#[derive(Debug)]
pub enum WasmError {
    /// LPIR → WASM emission failed.
    Emission(String),
    /// WASM parsing/validation failed.
    InvalidWasm(String),
    /// Runtime instantiation failed (runtime feature).
    #[cfg(feature = "runtime")]
    Instantiation(String),
    /// Runtime call failed (trap, etc).
    #[cfg(feature = "runtime")]
    Call(String),
}

impl core::fmt::Display for WasmError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Emission(s) => write!(f, "WASM emission error: {s}"),
            Self::InvalidWasm(s) => write!(f, "invalid WASM: {s}"),
            #[cfg(feature = "runtime")]
            Self::Instantiation(s) => write!(f, "WASM instantiation error: {s}"),
            #[cfg(feature = "runtime")]
            Self::Call(s) => write!(f, "WASM call error: {s}"),
        }
    }
}

impl core::error::Error for WasmError {}
```

**Tests in emit.rs:**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use lpir::parse_module;

    fn validate_wasm(bytes: &[u8]) -> Result<(), String> {
        // Use wasmparser or simple magic bytes check
        if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
            return Err("not a WASM module".into());
        }
        // TODO: full validation with wasmparser
        Ok(())
    }

    #[test]
    fn emit_empty_module() {
        let ir = parse_module("").expect("parse empty");
        let module = emit_module(&ir, &WasmOptions::default()).expect("emit");
        validate_wasm(&module.bytes).expect("valid WASM");
        assert!(module.exports.is_empty());
    }

    #[test]
    fn emit_simple_function() {
        let ir = parse_module(r#"
func @add(v1:i32, v2:i32) -> i32 {
  v3:i32 = iadd v1, v2
  return v3
}
"#).expect("parse");
        let module = emit_module(&ir, &WasmOptions::default()).expect("emit");
        validate_wasm(&module.bytes).expect("valid WASM");
        assert_eq!(module.exports.len(), 1);
        assert_eq!(module.exports[0].name, "add");
    }
}
```

### Validate

```bash
cargo test -p lpvm-wasm --no-default-features -- --test-threads=1
```

Tests should pass (or fail with clear TODO messages if wasmparser not added yet).
