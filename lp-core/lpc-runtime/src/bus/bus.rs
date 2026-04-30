//! Bus — runtime registry of bus channels.

use crate::bus::bus_error::BusError;
use crate::bus::channel_entry::ChannelEntry;
use alloc::collections::BTreeMap;
use lpc_model::{ChannelName, FrameId, Kind, LpsValue, NodeId, PropPath};

/// Runtime registry of bus channels.
///
/// Channels exist when at least one binding references them
/// (lazy creation). The first claim establishes the channel's
/// `Kind`; subsequent claims with mismatched kinds fail. Per
/// design 06.
///
/// M4.2 ships the data + 5-method API; M4.3 wires it through
/// `TickContext`. Multi-bus topology and external-writer
/// registration are deferred.
#[derive(Default)]
pub struct Bus {
    channels: BTreeMap<ChannelName, ChannelEntry>,
}

impl Bus {
    pub fn new() -> Self {
        Self {
            channels: BTreeMap::new(),
        }
    }

    /// Establish or update a writer for `channel`. On first claim,
    /// sets the channel's `kind`. On subsequent claims with a
    /// mismatched `kind`, returns `Err(KindMismatch)` and leaves
    /// the channel unchanged.
    ///
    /// Replaces any existing `(writer_node, writer_prop)` pair —
    /// "last writer wins" for now (multi-writer arbitration is
    /// future work).
    pub fn claim_writer(
        &mut self,
        channel: &ChannelName,
        writer: NodeId,
        prop: PropPath,
        kind: Kind,
    ) -> Result<(), BusError> {
        let entry = self.channels.entry(channel.clone()).or_default();
        match entry.kind {
            Some(established) if established != kind => Err(BusError::KindMismatch {
                channel: channel.clone(),
                established,
                attempted: kind,
            }),
            _ => {
                entry.kind = Some(kind);
                entry.writer = Some((writer, prop));
                Ok(())
            }
        }
    }

    /// Publish `value` on `channel`. Bumps `last_writer_frame` and
    /// replaces `last_value`. No-op if no writer has been claimed
    /// for `channel`.
    pub fn publish(&mut self, channel: &ChannelName, value: LpsValue, frame: FrameId) {
        if let Some(entry) = self.channels.get_mut(channel) {
            if entry.writer.is_some() {
                entry.last_value = Some(value);
                entry.last_writer_frame = frame;
            }
        }
    }

    /// Read the last published value, cross-tick stable.
    pub fn read(&self, channel: &ChannelName) -> Option<&LpsValue> {
        self.channels
            .get(channel)
            .and_then(|e| e.last_value.as_ref())
    }

    /// Frame at which the channel was last written. `FrameId::new(0)`
    /// if the channel has never been written.
    pub fn last_writer_frame(&self, channel: &ChannelName) -> FrameId {
        self.channels
            .get(channel)
            .map(|e| e.last_writer_frame)
            .unwrap_or_else(FrameId::default)
    }

    /// The channel's established `Kind`, if any.
    pub fn kind(&self, channel: &ChannelName) -> Option<Kind> {
        self.channels.get(channel).and_then(|e| e.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::{Bus, BusError, ChannelName, FrameId, Kind, LpsValue, NodeId};
    use lpc_model::PropPath;

    fn ch(name: &str) -> ChannelName {
        ChannelName(alloc::string::String::from(name))
    }

    fn path(s: &str) -> PropPath {
        lpc_model::prop::prop_path::parse_path(s).unwrap()
    }

    #[test]
    fn bus_claim_publish_read_round_trip() {
        let mut bus = Bus::new();
        let channel = ch("speed");

        // Claim writer
        bus.claim_writer(
            &channel,
            NodeId::new(1),
            path("outputs[0]"),
            Kind::Amplitude,
        )
        .unwrap();

        // Publish
        bus.publish(&channel, LpsValue::F32(3.5), FrameId::new(10));

        // Read
        let val = bus.read(&channel).unwrap();
        assert!(matches!(val, LpsValue::F32(3.5)));
        assert_eq!(bus.last_writer_frame(&channel).as_i64(), 10);
    }

    #[test]
    fn bus_claim_writer_same_kind_succeeds() {
        let mut bus = Bus::new();
        let channel = ch("param");

        bus.claim_writer(&channel, NodeId::new(1), path("a"), Kind::Ratio)
            .unwrap();
        bus.claim_writer(&channel, NodeId::new(2), path("b"), Kind::Ratio)
            .unwrap();

        // Last writer wins
        let entry = bus.channels.get(&channel).unwrap();
        assert_eq!(entry.writer.as_ref().unwrap().0.as_u32(), 2);
    }

    #[test]
    fn bus_claim_writer_mismatched_kind_fails() {
        let mut bus = Bus::new();
        let channel = ch("mixed");

        bus.claim_writer(&channel, NodeId::new(1), path("a"), Kind::Amplitude)
            .unwrap();

        let result = bus.claim_writer(&channel, NodeId::new(2), path("b"), Kind::Ratio);
        assert!(matches!(result, Err(BusError::KindMismatch { .. })));

        // Channel unchanged
        let entry = bus.channels.get(&channel).unwrap();
        assert_eq!(entry.writer.as_ref().unwrap().0.as_u32(), 1);
        assert_eq!(entry.kind, Some(Kind::Amplitude));
    }

    #[test]
    fn bus_publish_before_claim_is_no_op() {
        let mut bus = Bus::new();
        let channel = ch("unclaimed");

        bus.publish(&channel, LpsValue::F32(5.0), FrameId::new(5));

        assert!(bus.read(&channel).is_none());
        assert_eq!(bus.last_writer_frame(&channel).as_i64(), 0);
    }

    #[test]
    fn bus_read_unknown_channel_returns_none() {
        let bus = Bus::new();
        assert!(bus.read(&ch("unknown")).is_none());
    }

    #[test]
    fn bus_kind_unknown_channel_returns_none() {
        let bus = Bus::new();
        assert!(bus.kind(&ch("unknown")).is_none());
    }

    #[test]
    fn bus_last_writer_frame_unknown_channel_returns_zero() {
        let bus = Bus::new();
        assert_eq!(bus.last_writer_frame(&ch("unknown")).as_i64(), 0);
    }

    #[test]
    fn bus_kind_returns_established_kind() {
        let mut bus = Bus::new();
        let channel = ch("typed");

        bus.claim_writer(&channel, NodeId::new(1), path("out"), Kind::Frequency)
            .unwrap();

        assert_eq!(bus.kind(&channel), Some(Kind::Frequency));
    }
}
