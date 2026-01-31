# Phase 2: Create Binary Building Helper Utility

## Scope of phase

Extract and generalize the binary building code from `guest_app_tests.rs` into a reusable utility module that can be used by multiple tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create test_util module (`lp-riscv/lp-riscv-emu/src/test_util.rs`)

```rust
//! Test utilities for building and loading RISC-V binaries

use std::path::PathBuf;
use std::sync::Mutex;

/// Configuration for building a RISC-V binary
#[derive(Debug, Clone)]
pub struct BinaryBuildConfig {
    /// Package name to build
    pub package: String,
    /// Target triple (e.g., "riscv32imac-unknown-none-elf")
    pub target: String,
    /// Rust flags (e.g., "-C target-feature=-c")
    pub rustflags: Option<String>,
    /// Build profile ("debug" or "release")
    pub profile: String,
}

impl BinaryBuildConfig {
    /// Create a new build config with defaults
    pub fn new(package: impl Into<String>) -> Self {
        Self {
            package: package.into(),
            target: "riscv32imac-unknown-none-elf".to_string(),
            rustflags: Some("-C target-feature=-c".to_string()),
            profile: "release".to_string(),
        }
    }

    /// Set the target triple
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = target.into();
        self
    }

    /// Set rustflags
    pub fn with_rustflags(mut self, rustflags: impl Into<String>) -> Self {
        self.rustflags = Some(rustflags.into());
        self
    }

    /// Set build profile
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = profile.into();
        self
    }
}

/// Cache for built binary paths (package -> path)
static BINARY_CACHE: Mutex<std::collections::HashMap<String, Option<PathBuf>>> = Mutex::new(std::collections::HashMap::new());

/// Ensure a binary is built and return its path
///
/// Builds the binary if not already built (or if cached path doesn't exist).
/// Caches the result to avoid rebuilding on subsequent calls.
///
/// # Arguments
/// * `config` - Build configuration
///
/// # Returns
/// * `Ok(PathBuf)` - Path to built binary
/// * `Err(String)` - Error message if build failed
pub fn ensure_binary_built(config: BinaryBuildConfig) -> Result<PathBuf, String> {
    let cache_key = format!("{}-{}-{}", config.package, config.target, config.profile);
    
    // Check cache first
    {
        let cache = BINARY_CACHE.lock().unwrap();
        if let Some(Some(ref path)) = cache.get(&cache_key) {
            if path.exists() {
                return Ok(path.clone());
            }
        }
    }

    // Find workspace root
    let workspace_root = find_workspace_root()
        .ok_or_else(|| "Failed to find workspace root".to_string())?;

    // Build binary
    println!("Building {} for {}...", config.package, config.target);
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(&workspace_root);
    
    if let Some(ref rustflags) = config.rustflags {
        cmd.env("RUSTFLAGS", rustflags);
    }
    
    cmd.args([
        "build",
        "--package",
        &config.package,
        "--target",
        &config.target,
    ]);
    
    if config.profile == "release" {
        cmd.arg("--release");
    }
    
    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute cargo build: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Build failed:\n{stderr}"));
    }

    // Determine expected binary path
    let exe_name = config.package.clone();
    let exe_path = workspace_root
        .join("target")
        .join(&config.target)
        .join(&config.profile)
        .join(&exe_name);

    if !exe_path.exists() {
        return Err(format!("Binary not found at: {}", exe_path.display()));
    }

    // Cache the path
    {
        let mut cache = BINARY_CACHE.lock().unwrap();
        cache.insert(cache_key, Some(exe_path.clone()));
    }

    Ok(exe_path)
}

/// Find workspace root by looking for Cargo.toml with [workspace]
///
/// Starts from current directory and walks up until finding a Cargo.toml
/// that contains "[workspace]".
///
/// # Returns
/// * `Some(PathBuf)` - Workspace root path
/// * `None` - Workspace root not found
pub fn find_workspace_root() -> Option<PathBuf> {
    let mut current_dir = std::env::current_dir().ok()?;
    loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
                if contents.contains("[workspace]") {
                    return Some(current_dir);
                }
            }
        }
        if let Some(parent) = current_dir.parent() {
            current_dir = parent.to_path_buf();
        } else {
            return None;
        }
    }
}
```

### 2. Export test_util module (`lp-riscv/lp-riscv-emu/src/lib.rs`)

```rust
#[cfg(feature = "std")]
pub mod test_util;

#[cfg(feature = "std")]
pub use test_util::{BinaryBuildConfig, ensure_binary_built, find_workspace_root};
```

### 3. Update `guest_app_tests.rs` to use the helper

Update the test to use the new helper:

```rust
use lp_riscv_emu::test_util::{BinaryBuildConfig, ensure_binary_built};

fn ensure_test_app_bin() -> Result<std::path::PathBuf, String> {
    let config = BinaryBuildConfig::new("lp-riscv-emu-guest-test-app");
    ensure_binary_built(config)
}
```

## Tests

Add tests for the helper functions:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_workspace_root() {
        let root = find_workspace_root();
        assert!(root.is_some(), "Should find workspace root");
        let root = root.unwrap();
        assert!(root.join("Cargo.toml").exists(), "Root should have Cargo.toml");
    }

    #[test]
    fn test_binary_build_config_defaults() {
        let config = BinaryBuildConfig::new("test-package");
        assert_eq!(config.package, "test-package");
        assert_eq!(config.target, "riscv32imac-unknown-none-elf");
        assert_eq!(config.profile, "release");
        assert!(config.rustflags.is_some());
    }
}
```

## Validate

Run from `lp-riscv/lp-riscv-emu/` directory:

```bash
cd lp-riscv/lp-riscv-emu
cargo test --lib test_util
cargo check
```

Ensure:
- Helper functions compile
- `guest_app_tests.rs` can use the helper
- Tests pass
- No warnings (except for TODO comments if any)
