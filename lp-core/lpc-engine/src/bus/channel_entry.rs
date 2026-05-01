//! ChannelEntry — per-channel state on the bus.

use lpc_model::{FrameId, Kind, NodeId, PropPath};
use lps_shared::LpsValueF32;

/// Per-channel state on the bus.
#[derive(Clone, Debug)]
pub struct ChannelEntry {
    /// The current writer for this channel, if any. Set by
    /// `Bus::claim_writer`.
    pub writer: Option<(NodeId, PropPath)>,

    /// Last value published to this channel. `None` means no
    /// publish has happened yet.
    pub last_value: Option<LpsValueF32>,

    /// Frame at which `last_value` was published. `FrameId::new(0)`
    /// if never written.
    pub last_writer_frame: FrameId,

    /// Channel kind, established by first reader/writer claim.
    /// `None` until the first claim.
    pub kind: Option<Kind>,
}

impl Default for ChannelEntry {
    fn default() -> Self {
        Self {
            writer: None,
            last_value: None,
            last_writer_frame: FrameId::new(0),
            kind: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ChannelEntry;

    #[test]
    fn channel_entry_default_is_empty() {
        let entry: ChannelEntry = Default::default();
        assert!(entry.writer.is_none());
        assert!(entry.last_value.is_none());
        assert!(entry.kind.is_none());
        assert_eq!(entry.last_writer_frame.as_i64(), 0);
    }

    #[test]
    fn channel_entry_clone_preserves_values() {
        let entry = ChannelEntry::default();
        let cloned = entry.clone();
        assert!(cloned.writer.is_none());
        assert_eq!(cloned.last_writer_frame.as_i64(), 0);
    }

    #[test]
    fn channel_entry_debug_prints() {
        let entry = ChannelEntry::default();
        let s = alloc::format!("{entry:?}");
        assert!(s.contains("ChannelEntry"));
    }
}
