use alloc::vec::Vec;

use lpc_model::{ChannelName, FrameId, Kind, NodeId, PropPath};
use lpc_source::SrcValueSpec;

use super::BindingId;

/// Input to [`crate::binding::BindingRegistry::register`]: all fields of a
/// [`BindingEntry`] except assigned id and version (the frame sets version).
#[derive(Clone, Debug)]
pub struct BindingDraft {
    pub source: BindingSource,
    pub target: BindingTarget,
    pub priority: BindingPriority,
    pub kind: Kind,
    pub owner: NodeId,
}

/// One registered binding: identity, endpoints, priority, kind, and version.
#[derive(Clone, Debug)]
pub struct BindingEntry {
    pub id: BindingId,
    pub source: BindingSource,
    pub target: BindingTarget,
    pub priority: BindingPriority,
    pub kind: Kind,
    pub version: FrameId,
    pub owner: NodeId,
}

/// Where a binding reads from.
#[derive(Clone, Debug)]
pub enum BindingSource {
    Literal(SrcValueSpec),
    NodeOutput { node: NodeId, output: PropPath },
    BusChannel(ChannelName),
}

/// Where a binding writes to.
#[derive(Clone, Debug)]
pub enum BindingTarget {
    NodeInput { node: NodeId, input: PropPath },
    NodeOutput { node: NodeId, output: PropPath },
    BusChannel(ChannelName),
}

/// Writer priority for the same bus channel; higher wins at resolution time.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingPriority(pub i32);

impl BindingPriority {
    pub fn new(p: i32) -> Self {
        Self(p)
    }

    pub fn as_i32(self) -> i32 {
        self.0
    }
}

impl core::fmt::Display for BindingPriority {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub(crate) fn channels_touched(source: &BindingSource, target: &BindingTarget) -> Vec<ChannelName> {
    let mut channels = Vec::new();
    if let BindingSource::BusChannel(c) = source {
        channels.push(c.clone());
    }
    if let BindingTarget::BusChannel(c) = target {
        channels.push(c.clone());
    }
    channels.sort();
    channels.dedup();
    channels
}
