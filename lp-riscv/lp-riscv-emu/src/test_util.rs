//! Test utilities for building and loading RISC-V binaries

#[cfg(feature = "std")]
mod std_impl {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::string::{String, ToString};
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

    /// Cache for built binary paths (cache_key -> path)
    fn get_cache() -> &'static Mutex<HashMap<String, Option<PathBuf>>> {
        use std::sync::OnceLock;
        static CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

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
        let cache_key = std::format!("{}-{}-{}", config.package, config.target, config.profile);

        // Check cache first
        {
            let cache = get_cache().lock().unwrap();
            if let Some(Some(path)) = cache.get(&cache_key) {
                if path.exists() {
                    return Ok(path.clone());
                }
            }
        }

        // Find workspace root
        let workspace_root =
            find_workspace_root().ok_or_else(|| "Failed to find workspace root".to_string())?;

        // Build binary
        std::println!("Building {} for {}...", config.package, config.target);
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
            .map_err(|e| std::format!("Failed to execute cargo build: {e}"))?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            
            // Print errors directly to stderr for better visibility
            std::eprintln!("=== Build failed for {} ===", config.package);
            if !stdout.is_empty() {
                std::eprintln!("--- stdout ---");
                std::eprintln!("{}", stdout);
            }
            if !stderr.is_empty() {
                std::eprintln!("--- stderr ---");
                std::eprintln!("{}", stderr);
            }
            
            return Err(std::format!(
                "Build failed for {} (exit code: {})\nSee stderr above for details.",
                config.package,
                output.status.code().unwrap_or(-1)
            ));
        }

        // Determine expected binary path
        let exe_name = config.package.clone();
        let exe_path = workspace_root
            .join("target")
            .join(&config.target)
            .join(&config.profile)
            .join(&exe_name);

        if !exe_path.exists() {
            return Err(std::format!("Binary not found at: {}", exe_path.display()));
        }

        // Cache the path
        {
            let mut cache = get_cache().lock().unwrap();
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
}

#[cfg(feature = "std")]
pub use std_impl::{BinaryBuildConfig, ensure_binary_built, find_workspace_root};

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {
    use super::*;

    #[test]
    fn test_find_workspace_root() {
        let root = find_workspace_root();
        assert!(root.is_some(), "Should find workspace root");
        let root = root.unwrap();
        assert!(
            root.join("Cargo.toml").exists(),
            "Root should have Cargo.toml"
        );
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
