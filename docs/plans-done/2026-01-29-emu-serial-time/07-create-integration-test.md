# Phase 7: Create integration test

## Scope of phase

Create an integration test that runs the emulator with `lp-riscv-emu-guest-test-app`, handles serial
communication, and verifies the serial and time functionality works correctly. The test will have a
main loop that runs the emulator until yield, processes serial messages, and repeats.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later

## Implementation Details

### 1. Create `lp-riscv/lp-riscv-tools/tests/integration_fw_emu.rs`

```rust
//! Integration test for emulator serial and time functionality
//!
//! Tests that the emulator can:
//! - Run firmware that uses serial I/O
//! - Handle yield syscalls
//! - Track time correctly
//! - Communicate via serial buffers

#[cfg(feature = "std")]
mod tests {
    use lp_riscv_tools::emu::{Riscv32Emulator, StepResult};
    use std::path::PathBuf;
    use std::fs;

    /// Find the emu-guest-test-app executable
    fn find_test_app_executable() -> Option<Vec<u8>> {
        let target = "riscv32imac-unknown-none-elf";
        let profile = "release";

        // Try to find workspace root
        let mut current_dir = std::env::current_dir().ok()?;
        loop {
            let cargo_toml = current_dir.join("Cargo.toml");
            if cargo_toml.exists() {
                if let Ok(contents) = std::fs::read_to_string(&cargo_toml) {
                    if contents.contains("[workspace]") {
                        break;
                    }
                }
            }
            if let Some(parent) = current_dir.parent() {
                current_dir = parent.to_path_buf();
            } else {
                return None;
            }
        }

        // Path to the executable
        let exe_path = current_dir
            .join("target")
            .join(target)
            .join(profile)
            .join("lp-riscv-emu-guest-test-app");

        if exe_path.exists() {
            return std::fs::read(&exe_path).ok();
        }

        None
    }

    /// Build the test app if it doesn't exist
    fn ensure_test_app_built() -> Result<Vec<u8>, String> {
        // First try to find existing binary
        if let Some(binary) = find_test_app_executable() {
            return Ok(binary);
        }

        // Build it
        println!("Building lp-riscv-emu-guest-test-app...");
        let output = std::process::Command::new("cargo")
            .args([
                "build",
                "--package",
                "lp-riscv-emu-guest-test-app",
                "--target",
                "riscv32imac-unknown-none-elf",
                "--release",
            ])
            .output()
            .map_err(|e| format!("Failed to build: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Build failed:\n{stderr}"));
        }

        // Try to find it again
        find_test_app_executable()
            .ok_or_else(|| "Binary not found after build".to_string())
    }

    #[test]
    fn test_serial_echo() {
        // Build or find test app
        let binary = ensure_test_app_built().expect("Failed to build test app");

        // Load ELF (simplified - we'll need ELF loading)
        // For now, assume we can create emulator with binary
        // TODO: Use proper ELF loading once available

        // Create emulator with fuel limit
        let mut emu = Riscv32Emulator::new(binary.clone(), vec![]);
        emu.set_max_instructions(1_000_000); // 1M instructions max

        // Send "echo hello" command
        emu.add_serial_input(b"echo hello\n");

        // Run until yield or fuel runs out
        let mut yield_count = 0;
        let mut output_received = false;

        loop {
            match emu.step() {
                Ok(StepResult::Syscall(info)) if info.number == 4 => {
                    // Yield syscall
                    yield_count += 1;

                    // Check for output
                    let output = emu.drain_serial_output();
                    if !output.is_empty() {
                        let output_str = String::from_utf8_lossy(&output);
                        if output_str.contains("echo: hello") {
                            output_received = true;
                            break;
                        }
                    }

                    // Continue execution
                    continue;
                }
                Ok(StepResult::Halted) => {
                    // Emulator halted
                    break;
                }
                Ok(StepResult::Continue) => {
                    continue;
                }
                Ok(StepResult::Panic(info)) => {
                    panic!("Emulator panicked: {}", info.message);
                }
                Ok(StepResult::Trap(code)) => {
                    panic!("Emulator trapped: {:?}", code);
                }
                Err(e) => {
                    panic!("Emulator error: {:?}", e);
                }
                _ => {
                    continue;
                }
            }
        }

        assert!(output_received, "Expected echo output but didn't receive it");
        assert!(yield_count > 0, "Expected at least one yield");
    }

    #[test]
    fn test_time_syscall() {
        // Build or find test app
        let binary = ensure_test_app_built().expect("Failed to build test app");

        // Create emulator
        let mut emu = Riscv32Emulator::new(binary.clone(), vec![]);
        emu.set_max_instructions(1_000_000);

        // Send "time" command
        emu.add_serial_input(b"time\n");

        // Run and check output
        let mut time_received = false;

        loop {
            match emu.step() {
                Ok(StepResult::Syscall(info)) if info.number == 4 => {
                    // Yield - check output
                    let output = emu.drain_serial_output();
                    if !output.is_empty() {
                        let output_str = String::from_utf8_lossy(&output);
                        if output_str.contains("time:") {
                            time_received = true;
                            // Verify it's a number
                            assert!(output_str.matches(char::is_numeric).count() > 0);
                            break;
                        }
                    }
                    continue;
                }
                Ok(StepResult::Halted) => break,
                Ok(StepResult::Continue) => continue,
                Ok(StepResult::Panic(info)) => panic!("Panic: {}", info.message),
                Err(e) => panic!("Error: {:?}", e),
                _ => continue,
            }
        }

        assert!(time_received, "Expected time output");
    }
}
```

**Note**: This is a simplified version. The actual implementation will need:

1. Proper ELF loading (use existing ELF loader from `lp-riscv-tools`)
2. Better error handling
3. More comprehensive test cases
4. Proper handling of the emulator entry point

### 2. Use existing ELF loader

Check if there's an ELF loader available in `lp-riscv-tools` and use it to load the binary properly.

## Validate

Run from workspace root:

```bash
# Ensure test app is built
just build-rv32-emu-guest-test-app

# Run integration test
cargo test --package lp-riscv-tools --test integration_fw_emu
```

Ensure:

- Test app builds before test runs
- Integration test passes
- Serial communication works correctly
- Time syscall works correctly
- Yield syscall works correctly
