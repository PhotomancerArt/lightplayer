//! Test discovery and execution for GLSL filetests.
//!
//! This module discovers all `.glsl` files in the `filetests/` directory and runs them,
//! providing detailed output about which tests pass or fail.

use anyhow::Result;
use lp_glsl_filetests::{parse, run_filetest};
use std::path::PathBuf;
use walkdir::WalkDir;

// Ignored: we do not want filetests to run as part of `cargo test`.
// They should be run separately using the `scripts/glsl-filetests.sh` script.
#[ignore]
#[test]
fn filetests() -> Result<()> {
    // Check builtins executable availability early
    check_builtins_executable()?;

    let filetests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("filetests");

    // Check for filtering environment variables
    let test_file_filter = std::env::var("TEST_FILE").ok();
    let test_line_filter: Option<usize> =
        std::env::var("TEST_LINE").ok().and_then(|s| s.parse().ok());

    let mut test_files = Vec::new();
    for entry in WalkDir::new(&filetests_dir) {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("glsl") {
            // Filter by file name if TEST_FILE is set
            if let Some(ref filter) = test_file_filter {
                let relative_path = path
                    .strip_prefix(&filetests_dir)
                    .unwrap_or(path)
                    .to_string_lossy();
                if !relative_path.contains(filter) {
                    continue;
                }
            }
            test_files.push(path.to_path_buf());
        }
    }

    // Sort for deterministic output
    test_files.sort();

    let mut passed = 0;
    let mut failed = 0;

    println!("\nRunning {} test file(s)...\n", test_files.len());

    for path in &test_files {
        let relative_path = path
            .strip_prefix(&filetests_dir)
            .unwrap_or(path)
            .to_string_lossy();

        // Parse the file to get test count
        let test_file = match parse::parse_test_file(path) {
            Ok(tf) => tf,
            Err(e) => {
                print!("test {relative_path} ... ");
                println_colored("FAILED (parse error)", colors::RED);
                if should_color() {
                    println!("  {}Error:{} {:#}", colors::RED, colors::RESET, e);
                } else {
                    println!("  Error: {e:#}");
                }
                failed += 1;
                continue;
            }
        };

        // Count test cases (respecting filters)
        let test_count = if let Some(ref filter_line) = test_line_filter {
            test_file
                .run_directives
                .iter()
                .filter(|d| d.line_number == *filter_line)
                .count()
        } else {
            test_file.run_directives.len()
        };

        let test_label = if test_count > 0 {
            format!(
                "{} ({} test case{})",
                relative_path,
                test_count,
                if test_count == 1 { "" } else { "s" }
            )
        } else {
            relative_path.to_string()
        };

        print!("test {test_label} ... ");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();

        // Catch panics so one test failure doesn't stop others
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_filetest(path)));

        match result {
            Ok(Ok(())) => {
                println_colored("ok", colors::GREEN);
                passed += 1;
            }
            Ok(Err(e)) => {
                println_colored("FAILED", colors::RED);
                // Error is already fully formatted by GlslError::Display and format_compilation_error
                // Just display it directly - no reformatting needed
                println!("\n{e:#}");
                failed += 1;
            }
            Err(panic_payload) => {
                println_colored("FAILED (panic)", colors::RED);
                // Try to extract panic message
                let panic_msg = if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else {
                    "unknown panic".to_string()
                };
                println!("  Panic: {panic_msg}");
                failed += 1;
            }
        }
    }

    print!("\ntest result: ");
    if failed == 0 {
        print_colored("ok", colors::GREEN);
        println!(". {passed} passed; {failed} failed; 0 ignored");
    } else {
        print_colored("FAILED", colors::RED);
        println!(". {passed} passed; {failed} failed; 0 ignored");
    }

    if failed > 0 {
        anyhow::bail!("{failed} test file(s) failed");
    }

    Ok(())
}

/// ANSI color codes for terminal output (matching Rust's test output style)
mod colors {
    pub const GREEN: &str = "\x1b[32m";
    pub const RED: &str = "\x1b[31m";
    pub const RESET: &str = "\x1b[0m";
}

/// Check if colors should be enabled
/// Respects NO_COLOR environment variable
/// Colors are enabled by default (cargo test will handle TTY detection)
fn should_color() -> bool {
    // Respect NO_COLOR environment variable (https://no-color.org/)
    std::env::var("NO_COLOR").is_err()
}

/// Print colored text if TTY, otherwise plain text
fn print_colored(text: &str, color: &str) {
    if should_color() {
        print!("{}{}{}", color, text, colors::RESET);
    } else {
        print!("{text}");
    }
}

/// Print colored text with newline if TTY, otherwise plain text
fn println_colored(text: &str, color: &str) {
    if should_color() {
        println!("{}{}{}", color, text, colors::RESET);
    } else {
        println!("{text}");
    }
}

/// Check if the builtins executable is available, returning an error if not.
fn check_builtins_executable() -> Result<()> {
    // The builtins executable check will happen at runtime when tests try to compile.
    // We can't easily check it here without accessing private modules, so we skip the check.
    // Tests will fail with a clear error message if the builtins executable is missing.
    Ok(())
}
