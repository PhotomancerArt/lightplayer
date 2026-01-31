//! Output mode for filetest execution.

/// Output mode determines how much detail is shown in test output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    /// Minimal output, used for multiple tests (summary mode)
    Summary,
    /// Full output for single test (detail mode)
    Detail,
    /// Full output + debug sections (when DEBUG=1)
    Debug,
}

impl OutputMode {
    /// Determine output mode based on test count and DEBUG environment variable.
    pub fn from_test_count(test_count: usize) -> Self {
        let is_debug = std::env::var("DEBUG").unwrap_or_default() == "1";
        if test_count == 1 {
            if is_debug {
                OutputMode::Debug
            } else {
                OutputMode::Detail
            }
        } else {
            OutputMode::Summary
        }
    }

    /// Check if this mode should show debug sections (emulator state, v-code, CLIF).
    pub fn show_debug_sections(self) -> bool {
        matches!(self, OutputMode::Debug)
    }

    /// Check if this mode should show full output (not just summary).
    pub fn show_full_output(self) -> bool {
        matches!(self, OutputMode::Detail | OutputMode::Debug)
    }
}
