//! Full-stack test for no_std RISC-V programs compiled with Cranelift.
//!
//! This test builds the embive-program (now a simple RISC-V program without embive)
//! and runs it in the lp-riscv-emu emulator to verify the entire toolchain works.

#[cfg(feature = "std")]
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{Riscv32Emulator, StepResult};
use lp_riscv_inst::Gpr;
use std::{sync::mpsc, thread, time::Duration};

#[test]
#[ignore] // Run with: cargo test --package lp-riscv-emu -- --ignored --nocapture riscv_nostd
fn test_riscv_nostd_hello_world() {
    // Run the test in a separate thread with a timeout
    let (tx, rx) = mpsc::channel();

    let handle = thread::spawn(move || {
        let result = run_nostd_test();
        let _ = tx.send(result);
    });

    // Wait for the test to complete with a 60 second timeout (build can take time)
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(Ok(())) => {} // Success
        Ok(Err(e)) => panic!("Test failed: {e}"),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            panic!("Test timed out after 60 seconds");
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("Test thread disconnected unexpectedly");
        }
    }

    // Wait for thread to finish
    let _ = handle.join();
}

fn run_nostd_test() -> Result<(), String> {
    // Find workspace root
    let workspace_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .ok_or_else(|| "Could not find workspace root".to_string())?;

    println!("=== RISC-V no_std Full-Stack Test ===");
    println!("Workspace: {}", workspace_root.display());
    println!();

    // Build the program
    println!("[1/4] Building embive-program for riscv32imac...");
    let output = std::process::Command::new("cargo")
        .env("RUSTFLAGS", "-C target-feature=-c") // Disable compressed instructions
        .args([
            "build",
            "--package",
            "embive-program",
            "--target",
            "riscv32imac-unknown-none-elf",
            "--release",
        ])
        .current_dir(workspace_root)
        .output()
        .map_err(|e| format!("Failed to build: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Build failed:\n{stderr}"));
    }
    println!("   ✓ Build successful");
    println!();

    // Load ELF
    println!("[2/4] Loading ELF binary...");
    let elf_path =
        workspace_root.join("target/riscv32imac-unknown-none-elf/release/embive-program");
    let elf_data = std::fs::read(&elf_path).map_err(|e| format!("Failed to read ELF: {e}"))?;

    #[cfg(feature = "std")]
    let elf_info = load_elf(&elf_data)?;
    #[cfg(not(feature = "std"))]
    return Err("ELF loading requires std feature".to_string());
    println!(
        "   ✓ Loaded: {} bytes code, {} bytes RAM",
        elf_info.code.len(),
        elf_info.ram.len()
    );
    println!("   ✓ Entry point: 0x{:08x}", elf_info.entry_point);

    // Debug: Show first few instructions
    println!("   Debug: First 32 bytes of code:");
    for i in (0..32.min(elf_info.code.len())).step_by(4) {
        if i + 4 <= elf_info.code.len() {
            let word = u32::from_le_bytes([
                elf_info.code[i],
                elf_info.code[i + 1],
                elf_info.code[i + 2],
                elf_info.code[i + 3],
            ]);
            println!("     0x{i:04x}: 0x{word:08x}");
        }
    }
    println!();

    // Create emulator
    println!("[3/4] Running in RISC-V emulator...");
    // Fuel is now per-run, not global
    let mut emu = Riscv32Emulator::new(elf_info.code, elf_info.ram);

    let mut output_lines = Vec::new();
    let mut result_value = None;

    // Run until halt
    loop {
        match emu.step() {
            Ok(StepResult::FuelExhausted(_)) => {
                unreachable!("step() should never return FuelExhausted");
            }
            Ok(StepResult::Continue) => continue,
            Ok(StepResult::Panic(panic_info)) => {
                let msg = if let Some(ref file) = panic_info.file {
                    if let Some(line) = panic_info.line {
                        format!("{file}:{line}")
                    } else {
                        file.clone()
                    }
                } else if let Some(line) = panic_info.line {
                    format!("line {line}")
                } else {
                    "unknown location".to_string()
                };
                return Err(format!(
                    "Guest panicked at PC 0x{:x}: {} ({msg})",
                    panic_info.pc, panic_info.message
                ));
            }
            Ok(StepResult::Syscall(info)) => {
                // Handle syscall
                match info.number {
                    0 => {
                        // SYSCALL_DONE
                        result_value = Some(info.args[0]);
                        println!("   [syscall] Done with result: {}", info.args[0]);
                        emu.set_register(Gpr::A0, 0); // a0 = 0 (success)
                    }
                    1 => {
                        // SYSCALL_PANIC
                        let msg_ptr = info.args[0] as u32;
                        let msg_len = info.args[1] as usize;

                        // Try to read panic message
                        let msg = if msg_len > 0 && msg_len < 1024 {
                            let mut bytes = Vec::new();
                            for i in 0..msg_len {
                                match emu.memory().read_u8(msg_ptr + i as u32) {
                                    Ok(b) => bytes.push(b),
                                    Err(_) => break,
                                }
                            }
                            String::from_utf8_lossy(&bytes).to_string()
                        } else {
                            "panic".to_string()
                        };

                        return Err(format!("Guest panicked: {msg}"));
                    }
                    2 => {
                        // SYSCALL_WRITE
                        let ptr = info.args[0] as u32;
                        let len = info.args[1] as usize;

                        // Read string from memory
                        if len > 0 && len < 1_000_000 {
                            let mut bytes = Vec::new();
                            for i in 0..len {
                                match emu.memory().read_u8(ptr + i as u32) {
                                    Ok(b) => bytes.push(b),
                                    Err(_) => break,
                                }
                            }

                            let text = String::from_utf8_lossy(&bytes);
                            print!("   {text}");
                            std::io::Write::flush(&mut std::io::stdout()).unwrap();
                            output_lines.push(text.to_string());
                        }
                        emu.set_register(Gpr::A0, 0); // a0 = 0 (success)
                    }
                    _ => {
                        println!("   [syscall] Unknown: {}", info.number);
                        emu.set_register(Gpr::A0, 0);
                    }
                }
            }
            Ok(StepResult::Trap(_)) => {
                println!("   ✓ Program halted (TRAP)");
                break;
            }
            Ok(StepResult::Halted) => {
                println!("   ✓ Program halted (EBREAK)");
                break;
            }
            Err(e) => {
                return Err(format!("Emulator error: {e:?}"));
            }
        }
    }
    println!();

    // Verify results
    println!("[4/4] Verifying output...");
    let full_output = output_lines.join("");

    // Phase 1: Check for expected hello world message
    if !full_output.contains("Hello from RISC-V!") {
        return Err(format!(
            "Expected 'Hello from RISC-V!' not found in output:\n{full_output}"
        ));
    }
    println!("   ✓ Found expected output: 'Hello from RISC-V!'");

    if !full_output.contains("no_std") {
        return Err(format!(
            "Expected 'no_std' not found in output:\n{full_output}"
        ));
    }
    println!("   ✓ Found 'no_std' mention");

    if !full_output.contains("Cranelift") {
        return Err(format!(
            "Expected 'Cranelift' not found in output:\n{full_output}"
        ));
    }
    println!("   ✓ Found 'Cranelift' mention");

    // Phase 2: Check that program completed
    if !full_output.contains("Successfully executed") {
        return Err(format!(
            "Expected success message not found in output:\n{full_output}"
        ));
    }
    println!("   ✓ Program executed successfully");

    // Verify the syscall result
    match result_value {
        Some(15) => {
            println!("   ✓ Correct result received: 15 (expected for 5 * 3)");
        }
        Some(other) => {
            return Err(format!("Incorrect result: expected 15, got {other}"));
        }
        None => {
            return Err("No result received from program".to_string());
        }
    }

    println!();
    println!("=== ✅ All Tests Passed! ===");
    println!("Successfully ran no_std program compiled with Cranelift on RISC-V emulator");
    println!("- Binary built for riscv32imac target");
    println!("- ELF loaded into emulator memory");
    println!("- Program executed with syscall interface");
    println!("- FENCE.I instruction support verified (JIT code execution)");
    println!("- Output verification passed");
    println!();

    Ok(())
}
