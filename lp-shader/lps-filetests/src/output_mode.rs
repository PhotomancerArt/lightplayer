//! Output mode for filetest execution.
//!
//! Resolution (see [`OutputMode::resolve`]): optional CLI override, then `DEBUG=1`, then
//! single-file vs multi-file default.

/// Output mode determines how much detail is shown in test output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Concise output for directory test runs (one line per test, minimal details on failure)
    Concise,
    /// Full output for single file test execution (shows file contents and rerun instructions)
    Detail,
    /// Full output + debug sections (when DEBUG=1 or `--debug`)
    Debug,
}

impl OutputMode {
    /// Pick output mode for a test run.
    ///
    /// Precedence: `cli_override` if set, else `DEBUG=1` (`env_debug`), else one file →
    /// [`Detail`](OutputMode::Detail), multiple files → [`Concise`](OutputMode::Concise).
    pub fn resolve(cli_override: Option<OutputMode>, env_debug: bool, file_count: usize) -> Self {
        if let Some(mode) = cli_override {
            return mode;
        }
        if env_debug {
            return OutputMode::Debug;
        }
        if file_count == 1 {
            OutputMode::Detail
        } else {
            OutputMode::Concise
        }
    }

    /// True when `DEBUG` is set to `1` in the environment.
    pub fn env_wants_debug() -> bool {
        std::env::var("DEBUG").unwrap_or_default() == "1"
    }

    /// Check if this mode should show debug sections (emulator state, v-code, CLIF).
    pub fn show_debug_sections(self) -> bool {
        matches!(self, OutputMode::Debug)
    }

    /// Check if this mode should show full output.
    /// Returns true for Detail and Debug modes, false for Concise mode.
    pub fn show_full_output(self) -> bool {
        matches!(self, OutputMode::Detail | OutputMode::Debug)
    }
}

#[cfg(test)]
mod tests {
    use super::OutputMode;

    #[test]
    fn resolve_cli_override_wins_over_env_and_count() {
        assert_eq!(
            OutputMode::resolve(Some(OutputMode::Concise), true, 1),
            OutputMode::Concise
        );
        assert_eq!(
            OutputMode::resolve(Some(OutputMode::Detail), true, 99),
            OutputMode::Detail
        );
        assert_eq!(
            OutputMode::resolve(Some(OutputMode::Debug), false, 99),
            OutputMode::Debug
        );
    }

    #[test]
    fn resolve_env_debug_when_no_override() {
        assert_eq!(OutputMode::resolve(None, true, 1), OutputMode::Debug);
        assert_eq!(OutputMode::resolve(None, true, 50), OutputMode::Debug);
    }

    #[test]
    fn resolve_file_count_when_no_override_no_env() {
        assert_eq!(OutputMode::resolve(None, false, 1), OutputMode::Detail);
        assert_eq!(OutputMode::resolve(None, false, 2), OutputMode::Concise);
        assert_eq!(OutputMode::resolve(None, false, 0), OutputMode::Concise);
    }
}
