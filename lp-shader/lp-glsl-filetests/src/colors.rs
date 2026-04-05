//! ANSI color codes for terminal output (matching Rust's test output style).

/// Green color
pub const GREEN: &str = "\x1b[32m";
/// Light/bright green color
pub const LIGHT_GREEN: &str = "\x1b[92m";
/// Red color
pub const RED: &str = "\x1b[31m";
/// Yellow color
pub const YELLOW: &str = "\x1b[33m";
/// Dim/grey color
pub const DIM: &str = "\x1b[2m";
/// Bold text
pub const BOLD: &str = "\x1b[1m";
/// Reset color
pub const RESET: &str = "\x1b[0m";

/// Check if colors should be enabled.
/// Respects NO_COLOR environment variable.
pub fn should_color() -> bool {
    std::env::var("NO_COLOR").is_err()
}

/// Format text with color if colors are enabled.
pub fn colorize(text: &str, color: &str) -> String {
    if should_color() {
        format!("{color}{text}{RESET}")
    } else {
        text.to_string()
    }
}
