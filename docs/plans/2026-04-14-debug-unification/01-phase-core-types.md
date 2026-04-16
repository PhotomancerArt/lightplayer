# Phase 1: Core Debug Types in lpvm

## Scope

Create the `ModuleDebugInfo` and `FunctionDebugInfo` types in `lpvm` crate and add the `debug_info()` trait method to `LpvmModule`.

## Implementation Details

### 1. Create `lpvm/src/debug.rs`

```rust
//! Compilation debug information types.

use alloc::collections::BTreeMap;
use alloc::string::String;

/// Per-function compilation debug info.
#[derive(Clone, Debug, Default)]
pub struct FunctionDebugInfo {
    /// Function name.
    pub name: String,
    /// Static instruction count (from disassembly).
    pub inst_count: usize,
    /// Named sections. Standard keys: "interleaved", "disasm", "vinst", "liveness", "region".
    pub sections: BTreeMap<String, String>,
}

impl FunctionDebugInfo {
    /// Create new FunctionDebugInfo with just a name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            inst_count: 0,
            sections: BTreeMap::new(),
        }
    }

    /// Add a section.
    pub fn with_section(mut self, name: impl Into<String>, content: impl Into<String>) -> Self {
        self.sections.insert(name.into(), content.into());
        self
    }

    /// Set instruction count.
    pub fn with_inst_count(mut self, count: usize) -> Self {
        self.inst_count = count;
        self
    }
}

/// Module-level compilation debug info.
#[derive(Clone, Debug, Default)]
pub struct ModuleDebugInfo {
    /// Function name → debug info.
    pub functions: BTreeMap<String, FunctionDebugInfo>,
}

impl ModuleDebugInfo {
    /// Create empty ModuleDebugInfo.
    pub fn new() -> Self {
        Self {
            functions: BTreeMap::new(),
        }
    }

    /// Add a function's debug info.
    pub fn add_function(&mut self, info: FunctionDebugInfo) {
        self.functions.insert(info.name.clone(), info);
    }

    /// Render all functions or a filtered function to a string.
    pub fn render(&self, fn_filter: Option<&str>) -> String {
        // Implementation: iterate functions, print sections with headers
        // Skip "(not available...)" sections gracefully
    }

    /// Get list of function names.
    pub fn function_names(&self) -> Vec<&str> {
        self.functions.keys().map(|s| s.as_str()).collect()
    }

    /// Generate help text with copy-pasteable commands.
    pub fn help_text(&self, file_path: &str, target: &str) -> String {
        // Implementation: show available functions, copy-paste examples
    }
}
```

### 2. Update `lpvm/src/lib.rs`

Add module export:
```rust
pub mod debug;
pub use debug::{FunctionDebugInfo, ModuleDebugInfo};
```

### 3. Update `lpvm/src/module.rs`

Add trait method:
```rust
pub trait LpvmModule {
    type Instance: LpvmInstance;
    type Error: core::fmt::Display;

    fn signatures(&self) -> &LpsModuleSig;
    fn instantiate(&self) -> Result<Self::Instance, Self::Error>;

    /// Compilation debug info. Returns None if not available.
    fn debug_info(&self) -> Option<&ModuleDebugInfo> {
        None
    }
}
```

## Code Organization

- Place `debug.rs` near `module.rs` (logical grouping)
- Keep types simple - just data containers
- `render()` and `help_text()` are formatting logic, keep in the impl

## Tests

Add unit tests in `debug.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn function_debug_info_builder() {
        let info = FunctionDebugInfo::new("test")
            .with_inst_count(10)
            .with_section("disasm", "addi...");
        assert_eq!(info.name, "test");
        assert_eq!(info.inst_count, 10);
        assert!(info.sections.contains_key("disasm"));
    }

    #[test]
    fn module_debug_info_render_empty() {
        let module = ModuleDebugInfo::new();
        let output = module.render(None);
        assert!(output.is_empty() || output.contains("No functions"));
    }
}
```

## Validate

```bash
cargo check -p lpvm
cargo test -p lpvm
```
