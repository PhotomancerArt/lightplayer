//! Severity levels for Studio console log lines.

use crate::core::notice::UiNoticeLevel;

/// Severity for a Studio log line.
///
/// Variants are declared lowest-to-highest so the derived ordering makes the
/// console's display threshold a plain comparison:
/// `entry.level >= filter.min_level`.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum UiLogLevel {
    /// Very verbose diagnostic output, finer-grained than
    /// [`UiLogLevel::Debug`]. Hidden by every default filter.
    Trace,
    /// Verbose diagnostic output.
    Debug,
    /// Normal informational output.
    Info,
    /// Recoverable issue or attention-worthy output.
    Warn,
    /// Failed operation or serious problem.
    Error,
}

impl UiLogLevel {
    /// The log severity that corresponds to a notice severity.
    pub fn from_notice_level(level: UiNoticeLevel) -> Self {
        match level {
            UiNoticeLevel::Info => Self::Info,
            UiNoticeLevel::Warning => Self::Warn,
            UiNoticeLevel::Error => Self::Error,
        }
    }

    /// Lowercase display label ("trace" … "error"), used in log messages and
    /// level selectors.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_order_lowest_to_highest() {
        assert!(UiLogLevel::Trace < UiLogLevel::Debug);
        assert!(UiLogLevel::Debug < UiLogLevel::Info);
        assert!(UiLogLevel::Info < UiLogLevel::Warn);
        assert!(UiLogLevel::Warn < UiLogLevel::Error);
    }
}
