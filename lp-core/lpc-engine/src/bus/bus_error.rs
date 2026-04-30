//! BusError — errors from Bus operations.

use lpc_model::{ChannelName, Kind};

/// Errors from `Bus` operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BusError {
    /// `claim_writer` was called on a channel whose `kind` had
    /// already been established by a prior reader/writer, and the
    /// new claim's `kind` doesn't match.
    ///
    /// Per design 06: first reader/writer declares kind; subsequent
    /// users must match. M4.3's resolver maps this to
    /// `NodeStatus::Warn` and falls through to slot default.
    KindMismatch {
        channel: ChannelName,
        established: Kind,
        attempted: Kind,
    },
}

impl core::fmt::Display for BusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BusError::KindMismatch {
                channel,
                established,
                attempted,
            } => write!(
                f,
                "bus channel `{}` is already kind `{:?}`, cannot claim as `{:?}`",
                channel.0, established, attempted,
            ),
        }
    }
}

impl core::error::Error for BusError {}

#[cfg(test)]
mod tests {
    use super::{BusError, ChannelName, Kind};
    use alloc::string::String;

    #[test]
    fn bus_error_kind_mismatch_format() {
        let err = BusError::KindMismatch {
            channel: ChannelName(String::from("audio/in/0")),
            established: Kind::Amplitude,
            attempted: Kind::Ratio,
        };
        let msg = alloc::format!("{}", err);
        assert!(msg.contains("audio/in/0"));
        assert!(msg.contains("Amplitude"));
        assert!(msg.contains("Ratio"));
    }

    #[test]
    fn bus_error_clone_round_trip() {
        let err = BusError::KindMismatch {
            channel: ChannelName(String::from("video/in/0")),
            established: Kind::Texture,
            attempted: Kind::Amplitude,
        };
        let cloned = err.clone();
        assert_eq!(err, cloned);
    }

    #[test]
    fn bus_error_debug_prints() {
        let err = BusError::KindMismatch {
            channel: ChannelName(String::from("time")),
            established: Kind::Instant,
            attempted: Kind::Duration,
        };
        let s = alloc::format!("{:?}", err);
        assert!(s.contains("KindMismatch"));
        assert!(s.contains("time"));
    }
}
