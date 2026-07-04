//! Device-wide recovery level, derived from the ledger.

/// How degraded the device currently is.
///
/// Derived, never stored: red if any path is gated, yellow if any path is
/// under watch, green otherwise.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum RecoveryLevel {
    Green,
    Yellow,
    Red,
}

impl RecoveryLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Green => "green",
            Self::Yellow => "yellow",
            Self::Red => "red",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_order_by_severity() {
        assert!(RecoveryLevel::Green < RecoveryLevel::Yellow);
        assert!(RecoveryLevel::Yellow < RecoveryLevel::Red);
    }
}
