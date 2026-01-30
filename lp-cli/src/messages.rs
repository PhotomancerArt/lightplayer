//! User-friendly message formatting helpers
//!
//! This module provides functions for formatting success and error messages
//! with actionable next steps and copy-pasteable commands.

/// Print success message with next steps
pub fn print_success(message: &str, next_steps: &[&str]) {
    println!("✓ {message}");
    if !next_steps.is_empty() {
        println!("\nNext steps:");
        for step in next_steps {
            println!("  {step}");
        }
    }
}

/// Print error message with suggestions
pub fn print_error(message: &str, suggestions: &[&str]) {
    eprintln!("✗ {message}");
    if !suggestions.is_empty() {
        eprintln!();
        for suggestion in suggestions {
            eprintln!("  {suggestion}");
        }
    }
}

/// Print error and return an anyhow error
///
/// Convenience function that prints a formatted error message and then returns
/// an error for propagation.
pub fn print_error_and_return(message: &str, suggestions: &[&str]) -> anyhow::Error {
    print_error(message, suggestions);
    anyhow::anyhow!("{message}")
}

/// Format command for copy-paste (with proper quoting if needed)
pub fn format_command(cmd: &str) -> String {
    // Check if command contains spaces and needs quoting
    if cmd.contains(' ') {
        // Simple heuristic: if it contains spaces and doesn't already have quotes,
        // wrap the whole command in quotes
        // For more complex cases, we might want to quote individual arguments
        if !cmd.starts_with('"') && !cmd.starts_with('\'') {
            format!("\"{}\"", cmd.replace('"', "\\\""))
        } else {
            cmd.to_string()
        }
    } else {
        cmd.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_command_simple() {
        let cmd = "lp-cli create my-project";
        let formatted = format_command(cmd);
        assert_eq!(formatted, "\"lp-cli create my-project\"");
    }

    #[test]
    fn test_format_command_no_spaces() {
        let cmd = "lp-cli";
        let formatted = format_command(cmd);
        assert_eq!(formatted, "lp-cli");
    }

    #[test]
    fn test_format_command_already_quoted() {
        let cmd = "\"lp-cli create my-project\"";
        let formatted = format_command(cmd);
        assert_eq!(formatted, cmd);
    }

    #[test]
    fn test_format_command_with_quotes() {
        let cmd = "lp-cli create \"my project\"";
        let formatted = format_command(cmd);
        // Should escape inner quotes
        assert!(formatted.contains("\\\""));
    }
}
