//! Integration test for emulator serial and time functionality
//!
//! Tests that the emulator can:
//! - Run firmware that uses serial I/O
//! - Handle yield syscalls
//! - Track time correctly
//! - Communicate via serial buffers

mod tests {
    use lp_riscv_elf::load_elf;
    use lp_riscv_emu::{LogLevel, Riscv32Emulator};

    use std::sync::Mutex;

    #[test]
    #[ignore] // TODO emu: Test app build failing (package ID specification issue)
    fn test_serial_echo() {
        let mut emu = setup_emulator();
        emu.serial_write(b"echo hello\n");
        emu.run_until_yield(1_000_000).unwrap_or_else(|e| {
            println!("{}", emu.dump_state());
            println!("\n=== Instruction Log ===");
            println!("{}", emu.format_logs());
            panic!("Emulator error: {:?}", e);
        });

        let output = emu.serial_read_line();
        assert_eq!(output, "echo: hello");
    }

    #[test]
    #[ignore] // TODO emu: Test app build failing (package ID specification issue)
    fn test_time_initial() {
        let mut emu = setup_emulator();
        emu.serial_write(b"time\n");
        emu.run_until_yield(1_000_000).unwrap_or_else(|e| {
            println!("{}", emu.dump_state());
            println!("\n=== Instruction Log ===");
            println!("{}", emu.format_logs());
            panic!("Emulator error: {:?}", e);
        });

        let output = emu.serial_read_line();
        assert!(output.starts_with("time: "));
        assert!(output.ends_with(" ms"));

        // Extract the time value
        let time_str = output
            .strip_prefix("time: ")
            .unwrap()
            .strip_suffix(" ms")
            .unwrap();
        let time_ms: u64 = time_str.parse().expect("Failed to parse time value");

        // Initial time should be 0 or very small (within first few milliseconds)
        assert!(
            time_ms < 100,
            "Initial time should be small, got {} ms",
            time_ms
        );
    }

    #[test]
    #[ignore] // TODO emu: Test app build failing (package ID specification issue)
    fn test_time_increases() {
        let mut emu = setup_emulator();

        // Get first time reading
        emu.serial_write(b"time\n");
        emu.run_until_yield(1_000_000).unwrap_or_else(|e| {
            println!("{}", emu.dump_state());
            panic!("Emulator error: {:?}", e);
        });
        let output1 = emu.serial_read_line();
        let time_str1 = output1
            .strip_prefix("time: ")
            .unwrap()
            .strip_suffix(" ms")
            .unwrap();
        let time_ms1: u64 = time_str1.parse().expect("Failed to parse first time value");

        // Wait a bit (sleep for 50ms)
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Get second time reading
        emu.serial_write(b"time\n");
        emu.run_until_yield(1_000_000).unwrap_or_else(|e| {
            println!("{}", emu.dump_state());
            panic!("Emulator error: {:?}", e);
        });
        let output2 = emu.serial_read_line();
        let time_str2 = output2
            .strip_prefix("time: ")
            .unwrap()
            .strip_suffix(" ms")
            .unwrap();
        let time_ms2: u64 = time_str2
            .parse()
            .expect("Failed to parse second time value");

        // Second time should be greater than first
        assert!(
            time_ms2 > time_ms1,
            "Time should increase: {} -> {}",
            time_ms1,
            time_ms2
        );

        // Should have increased by at least 40ms (allowing some margin for test overhead)
        assert!(
            time_ms2 >= time_ms1 + 40,
            "Time should have increased by at least 40ms: {} -> {}",
            time_ms1,
            time_ms2
        );
    }

    #[test]
    #[ignore] // TODO emu: Test app build failing (package ID specification issue)
    fn test_time_multiple_calls() {
        let mut emu = setup_emulator();

        // Call time multiple times in quick succession
        let mut times = Vec::new();
        for _ in 0..5 {
            emu.serial_write(b"time\n");
            emu.run_until_yield(1_000_000).unwrap_or_else(|e| {
                println!("{}", emu.dump_state());
                panic!("Emulator error: {:?}", e);
            });
            let output = emu.serial_read_line();
            let time_str = output
                .strip_prefix("time: ")
                .unwrap()
                .strip_suffix(" ms")
                .unwrap();
            let time_ms: u64 = time_str.parse().expect("Failed to parse time value");
            times.push(time_ms);
        }

        // All times should be non-decreasing
        for i in 1..times.len() {
            assert!(
                times[i] >= times[i - 1],
                "Time should be non-decreasing: {} -> {}",
                times[i - 1],
                times[i]
            );
        }
    }

    static TEST_APP_PATH: Mutex<Option<std::path::PathBuf>> = Mutex::new(None);

    /// Ensure test app is built and return its path
    /// Rebuilds the app once per test execution (cached)
    fn ensure_test_app_bin() -> Result<std::path::PathBuf, String> {
        // Check cache first
        {
            let cached = TEST_APP_PATH.lock().unwrap();
            if let Some(ref path) = *cached {
                if path.exists() {
                    return Ok(path.clone());
                }
            }
        }

        // Find workspace root
        let workspace_root =
            find_workspace_root().ok_or_else(|| "Failed to find workspace root".to_string())?;

        let target = "riscv32imac-unknown-none-elf";
        let profile = "release";
        let exe_path = workspace_root
            .join("target")
            .join(target)
            .join(profile)
            .join("lp-emu-guest-test-app");

        // Always rebuild
        println!("Building lp-emu-guest-test-app...");
        let output = std::process::Command::new("cargo")
            .current_dir(&workspace_root)
            .env("RUSTFLAGS", "-C target-feature=-c") // Disable compressed instructions
            .args([
                "build",
                "--package",
                "lp-emu-guest-test-app",
                "--target",
                target,
                "--release",
            ])
            .output()
            .map_err(|e| format!("Failed to build: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            println!("lp-emu-guest-test-app build failed");
            println!("{}", stderr);
            return Err(format!("Build failed"));
        }

        if !exe_path.exists() {
            return Err(format!("Binary not found at: {}", exe_path.display()));
        }

        // Cache the path
        {
            let mut cached = TEST_APP_PATH.lock().unwrap();
            *cached = Some(exe_path.clone());
        }

        Ok(exe_path)
    }

    /// Find workspace root by looking for Cargo.toml with [workspace]
    fn find_workspace_root() -> Option<std::path::PathBuf> {
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

    /// Set up emulator with test app loaded
    /// Builds the test app, loads the ELF, creates emulator, and sets PC to entry point
    fn setup_emulator() -> Riscv32Emulator {
        // Build test app and get path
        let exe_path = ensure_test_app_bin().expect("Failed to build test app");
        let binary = std::fs::read(&exe_path).expect("Failed to read binary");

        // Load ELF
        let elf_info = load_elf(&binary).expect("Failed to load ELF");

        println!("Entry point: 0x{:08x}", elf_info.entry_point);
        println!("Code size: {} bytes", elf_info.code.len());
        println!("RAM size: {} bytes", elf_info.ram.len());

        // Debug: Show first few instructions before moving
        println!("First 16 bytes of code:");
        for i in 0..16.min(elf_info.code.len()) {
            print!("{:02x} ", elf_info.code[i]);
        }
        println!();

        // Check if first instruction is compressed (bits [1:0] != 0b11)
        if elf_info.code.len() >= 2 {
            let first_two_bytes = (elf_info.code[0] as u16) | ((elf_info.code[1] as u16) << 8);
            let is_compressed = (first_two_bytes & 0x3) != 0x3;
            println!(
                "First instruction (little-endian): 0x{:04x}, compressed: {}",
                first_two_bytes, is_compressed
            );
        }

        // Create emulator with fuel limit and logging enabled
        let mut emu =
            Riscv32Emulator::new(elf_info.code, elf_info.ram).with_log_level(LogLevel::None);
        // Fuel is now per-run, passed to run_until_yield()

        // Set PC to entry point
        emu.set_pc(elf_info.entry_point);
        println!("Set PC to: 0x{:08x}", emu.get_pc());

        emu
    }
}
