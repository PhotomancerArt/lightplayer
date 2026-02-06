//! Test output formatting with ANSI colors

/// ANSI color codes
mod colors {
    pub const GREEN: &str = "\x1b[32m";
    pub const RED: &str = "\x1b[31m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const DIM: &str = "\x1b[2m";
    pub const RESET: &str = "\x1b[0m";
}

/// Check if colors should be enabled
/// Respects NO_COLOR environment variable
fn should_color() -> bool {
    std::env::var("NO_COLOR").is_err()
}

/// Format text with color if colors are enabled
fn colorize(text: &str, color: &str) -> String {
    if should_color() {
        format!("{color}{text}{}", colors::RESET)
    } else {
        text.to_string()
    }
}

/// Check if debug mode is enabled (DEBUG=1)
pub fn is_debug_mode() -> bool {
    std::env::var("DEBUG").map(|v| v == "1").unwrap_or(false)
}

/// Print test header with separator line
pub fn print_test_header(name: &str) {
    eprintln!("\n{}", "═".repeat(55));
    eprintln!("{}", name);
    eprintln!("{}\n", "═".repeat(55));
}

/// Print a step with status symbol on the left
///
/// # Arguments
/// * `status` - Status symbol: "✓", "✗", or "-" (for in-progress)
/// * `message` - Main message
/// * `details` - Optional details to show in grey (e.g., port name, values)
pub fn print_step(status: &str, message: &str, details: Option<&str>) {
    let status_colored = match status {
        "✓" => colorize("✓", colors::GREEN),
        "✗" => colorize("✗", colors::RED),
        _ => status.to_string(),
    };

    if let Some(details) = details {
        let details_colored = colorize(details, colors::DIM);
        eprintln!("{} {} {}", status_colored, message, details_colored);
    } else {
        eprintln!("{} {}", status_colored, message);
    }
}

/// Print step with timing information
pub fn print_step_with_time(status: &str, message: &str, details: Option<&str>, elapsed_secs: f64) {
    let time_str = format!("({:.1}s)", elapsed_secs);
    let details_str = if let Some(d) = details {
        format!("{} {}", d, colorize(&time_str, colors::DIM))
    } else {
        colorize(&time_str, colors::DIM)
    };
    print_step(status, message, Some(&details_str));
}

/// Print command/response arrows (only in debug mode)
pub fn print_command(cmd: &str) {
    if is_debug_mode() {
        let cmd_colored = colorize(cmd.trim(), colors::DIM);
        eprintln!("  → {}", cmd_colored);
    }
}

pub fn print_response(resp: &str) {
    if is_debug_mode() {
        let resp_colored = colorize(resp.trim(), colors::DIM);
        eprintln!("  ← {}", resp_colored);
    }
}

/// Execute a step with timing
///
/// Executes the closure and prints the result with timing.
/// Returns the result from the closure.
///
/// On success: prints ✓ with timing (caller can override with more details if needed).
/// On failure: prints ✗ with error message.
pub fn execute_step<F, T, E>(message: &str, details: Option<&str>, f: F) -> Result<T, E>
where
    F: FnOnce() -> Result<T, E>,
    E: std::fmt::Display,
{
    let start = std::time::Instant::now();

    // Execute the closure
    let result = f();

    // Calculate elapsed time
    let elapsed = start.elapsed().as_secs_f64();

    // Print result
    match &result {
        Ok(_) => {
            // Print success with timing (caller can override with more details if needed)
            print_step_with_time("✓", message, details, elapsed);
        }
        Err(e) => {
            print_step("✗", message, details);
            eprintln!("  ERROR: {}", e);
        }
    }

    result
}

/// Print test summary
pub fn print_summary(passed: usize, total: usize) {
    eprintln!("\n{}", "═".repeat(55));
    if passed == total {
        eprintln!(
            "{}",
            colorize(
                &format!("All tests passed ({}/{})", passed, total),
                colors::GREEN
            )
        );
    } else {
        eprintln!(
            "{}",
            colorize(
                &format!("Tests passed: {}/{}", passed, total),
                colors::YELLOW
            )
        );
    }

    if !is_debug_mode() {
        eprintln!(
            "{}",
            colorize(
                "Note: Set DEBUG=1 to see detailed command output",
                colors::DIM
            )
        );
    }
    eprintln!("{}\n", "═".repeat(55));
}
