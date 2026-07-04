//! Platform-agnostic classification of why the system (re)started.

/// Why the current boot happened. Platform backends map their SoC-specific
/// reset reasons (e.g. ESP32 `SocResetReason`) into this.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ResetCause {
    /// Cold power-on. The persistent region is garbage by definition.
    PowerOn,
    /// User/tool-initiated reset (USB-UART/JTAG, flash tool). Not a crash:
    /// clears blame like a power-on would, by policy.
    UserReset,
    /// Our own software reset — the panic path wrote a crash record first.
    SoftwareReset,
    /// Hardware watchdog fired. There was no crash-time hook; the eagerly
    /// maintained frame stack is the blame record.
    WatchdogReset,
    /// Brownout — a power problem. The code path is not to blame.
    Brownout,
    /// Anything else / unmapped.
    Unknown,
}

impl ResetCause {
    /// Whether the previous run's failure evidence should be blamed on the
    /// code path that was executing.
    ///
    /// `Unknown` deliberately does NOT blame: an unmapped cause (e.g. a
    /// deep-sleep wake) creating false blame is worse than missing an exotic
    /// crash — explicit crash records are blamed regardless of this policy.
    pub fn blames_code(self) -> bool {
        match self {
            Self::SoftwareReset | Self::WatchdogReset => true,
            Self::PowerOn | Self::UserReset | Self::Brownout | Self::Unknown => false,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PowerOn => "power-on",
            Self::UserReset => "user-reset",
            Self::SoftwareReset => "software-reset",
            Self::WatchdogReset => "watchdog-reset",
            Self::Brownout => "brownout",
            Self::Unknown => "unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blame_policy_matches_design() {
        assert!(ResetCause::SoftwareReset.blames_code());
        assert!(ResetCause::WatchdogReset.blames_code());
        assert!(!ResetCause::Unknown.blames_code());
        assert!(!ResetCause::PowerOn.blames_code());
        assert!(!ResetCause::UserReset.blames_code());
        assert!(!ResetCause::Brownout.blames_code());
    }
}
