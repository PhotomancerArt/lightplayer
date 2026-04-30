//! Test utilities for building and loading RISC-V binaries

#[cfg(feature = "std")]
mod std_impl {
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::string::{String, ToString};
    use std::sync::Mutex;
    use std::vec::Vec;

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
        /// Cargo features to enable
        pub features: Vec<String>,
        /// Use -Z build-std=core,alloc (needed for panic=unwind on bare-metal)
        pub build_std: bool,
    }

    impl BinaryBuildConfig {
        /// Create a new build config with defaults
        pub fn new(package: impl Into<String>) -> Self {
            Self {
                package: package.into(),
                target: "riscv32imac-unknown-none-elf".to_string(),
                rustflags: Some("-C target-feature=-c".to_string()),
                profile: "release".to_string(),
                features: Vec::new(),
                build_std: false,
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

        /// Enable frame pointers for backtrace support (required for stack unwinding).
        ///
        /// Adds `-C force-frame-pointers=yes` to RUSTFLAGS when building the binary.
        pub fn with_backtrace_support(mut self, enable: bool) -> Self {
            if enable {
                let extra = " -C force-frame-pointers=yes";
                self.rustflags = Some(self.rustflags.unwrap_or_default() + extra);
            }
            self
        }

        /// Enable full unwinding support (catch_unwind, .eh_frame, landing pads).
        ///
        /// Adds -C panic=unwind (overrides target's abort), -C force-unwind-tables=yes,
        /// and -C force-frame-pointers=yes. Required for fw-emu unwind tests.
        pub fn with_unwind_support(mut self, enable: bool) -> Self {
            if enable {
                let extra = " -C panic=unwind -C force-unwind-tables=yes";
                self.rustflags = Some(self.rustflags.unwrap_or_default() + extra);
            }
            self
        }

        /// Use -Z build-std=core,alloc (required for panic=unwind on bare-metal targets).
        pub fn with_build_std(mut self, enable: bool) -> Self {
            self.build_std = enable;
            self
        }

        /// Add cargo features to enable when building.
        pub fn with_features(mut self, features: &[&str]) -> Self {
            self.features = features.iter().map(|s| s.to_string()).collect();
            self
        }
    }

    /// Cache for built binary paths (cache_key -> path)
    fn get_cache() -> &'static Mutex<HashMap<String, Option<PathBuf>>> {
        use std::sync::OnceLock;
        static CACHE: OnceLock<Mutex<HashMap<String, Option<PathBuf>>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(HashMap::new()))
    }

    /// Ensure a binary is built and return a stable cached copy of it.
    ///
    /// Uses a cross-process file lock to serialize builds, then copies the
    /// output to a feature-keyed cache path so concurrent builds with different
    /// feature sets don't stomp each other's binaries.
    pub fn ensure_binary_built(config: BinaryBuildConfig) -> Result<PathBuf, String> {
        let cache_key = build_cache_key(&config);

        let workspace_root =
            find_workspace_root().ok_or_else(|| "Failed to find workspace root".to_string())?;

        let cache_dir = workspace_root.join("target").join(".lp-test-cache");
        let cached_path = cache_dir.join(&cache_key);

        // Fast path: in-process cache hit
        {
            let cache = get_cache().lock().unwrap();
            if let Some(Some(path)) = cache.get(&cache_key) {
                if path.exists() {
                    return Ok(path.clone());
                }
            }
        }

        // Cross-process file lock to serialize all test builds
        let lock_path = workspace_root.join("target").join(".lp-build.lock");
        std::fs::create_dir_all(lock_path.parent().unwrap()).ok();
        let lock_file = std::fs::File::create(&lock_path)
            .map_err(|e| std::format!("Failed to create lock file: {e}"))?;
        lock_exclusive(&lock_file)
            .map_err(|e| std::format!("Failed to acquire build lock: {e}"))?;

        // Do not skip the build when a cached copy exists: the cache key does not include
        // dependency sources, so a stale binary would otherwise mask fixes in lp-engine / lpa-server.

        // Build binary
        std::println!("Building {} for {}...", config.package, config.target);
        run_cargo_build(&config, &workspace_root)?;

        // Copy to stable cache path
        let cargo_output = cargo_output_path(&config, &workspace_root);
        if !cargo_output.exists() {
            return Err(std::format!(
                "Binary not found at: {}",
                cargo_output.display()
            ));
        }

        std::fs::create_dir_all(&cache_dir)
            .map_err(|e| std::format!("Failed to create cache dir: {e}"))?;
        std::fs::copy(&cargo_output, &cached_path)
            .map_err(|e| std::format!("Failed to cache binary: {e}"))?;

        // Update in-process cache
        {
            let mut cache = get_cache().lock().unwrap();
            cache.insert(cache_key, Some(cached_path.clone()));
        }

        Ok(cached_path)
        // lock_file dropped here, releasing flock
    }

    fn build_cache_key(config: &BinaryBuildConfig) -> String {
        let rustflags_part = config.rustflags.as_deref().unwrap_or("");
        let features_part = config.features.join(",");
        std::format!(
            "{}-{}-{}-{}-{}-build_std={}",
            config.package,
            config.target,
            config.profile,
            rustflags_part.replace(' ', "_"),
            features_part,
            config.build_std
        )
    }

    fn cargo_output_path(config: &BinaryBuildConfig, workspace_root: &std::path::Path) -> PathBuf {
        workspace_root
            .join("target")
            .join(&config.target)
            .join(&config.profile)
            .join(&config.package)
    }

    fn run_cargo_build(
        config: &BinaryBuildConfig,
        workspace_root: &std::path::Path,
    ) -> Result<(), String> {
        let mut cmd = std::process::Command::new("cargo");
        cmd.current_dir(workspace_root);

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
        } else if config.profile != "debug" {
            cmd.args(["--profile", &config.profile]);
        }

        if !config.features.is_empty() {
            cmd.args(["--features", &config.features.join(",")]);
        }

        if config.build_std {
            cmd.args(["-Z", "build-std=core,alloc"]);
        }

        let output = cmd
            .output()
            .map_err(|e| std::format!("Failed to execute cargo build: {e}"))?;

        if !output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            std::eprintln!("=== Build failed for {} ===", config.package);
            if !stdout.is_empty() {
                std::eprintln!("--- stdout ---");
                std::eprintln!("{stdout}");
            }
            if !stderr.is_empty() {
                std::eprintln!("--- stderr ---");
                std::eprintln!("{stderr}");
            }

            return Err(std::format!(
                "Build failed for {} (exit code: {})\nSee stderr above for details.",
                config.package,
                output.status.code().unwrap_or(-1)
            ));
        }

        Ok(())
    }

    #[cfg(unix)]
    fn lock_exclusive(file: &std::fs::File) -> std::io::Result<()> {
        use std::os::unix::io::AsRawFd;
        let ret = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
        if ret != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    #[cfg(not(unix))]
    fn lock_exclusive(_file: &std::fs::File) -> std::io::Result<()> {
        Ok(())
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
