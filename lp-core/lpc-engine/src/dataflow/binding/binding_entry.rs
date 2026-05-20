use lpc_model::{ChannelName, Kind, LpValue, NodeId, Revision, SlotPath};

/// Stable address of a binding owned by a node entry.
///
/// Bindings are node-instance data, so their identity is local to the owning
/// node rather than allocated from a separate registry.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingRef {
    pub owner: NodeId,
    pub index: usize,
}

impl BindingRef {
    pub fn new(owner: NodeId, index: usize) -> Self {
        Self { owner, index }
    }
}

/// One registered binding: endpoints, priority, kind, and revision.
#[derive(Clone, Debug)]
pub struct BindingEntry {
    pub source: BindingSource,
    pub target: BindingTarget,
    pub priority: BindingPriority,
    pub kind: Kind,
    pub version: Revision,
    pub owner: NodeId,
}

/// Authored binding data before it is stored on its owner node.
#[derive(Clone, Debug)]
pub struct BindingDraft {
    pub source: BindingSource,
    pub target: BindingTarget,
    pub priority: BindingPriority,
    pub kind: Kind,
    pub owner: NodeId,
}

/// Where a binding reads from.
#[derive(Clone, Debug)]
pub enum BindingSource {
    Literal(LpValue),
    ProducedSlot { node: NodeId, slot: SlotPath },
    BusChannel(ChannelName),
}

/// Where a binding writes to.
#[derive(Clone, Debug)]
pub enum BindingTarget {
    ConsumedSlot { node: NodeId, slot: SlotPath },
    BusChannel(ChannelName),
}

/// Writer priority for the same bus channel; higher wins at resolution time.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct BindingPriority(pub i32);

impl BindingPriority {
    pub fn new(p: i32) -> Self {
        Self(p)
    }

    pub fn authored() -> Self {
        Self(0)
    }

    pub fn default_fallback() -> Self {
        Self(-1000)
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

pub(crate) fn channels_touched<'a>(
    source: &'a BindingSource,
    target: &'a BindingTarget,
) -> ChannelsTouched<'a> {
    let source = match source {
        BindingSource::BusChannel(channel) => Some(channel),
        _ => None,
    };
    let target = match target {
        BindingTarget::BusChannel(channel) => Some(channel),
        _ => None,
    };
    ChannelsTouched {
        source,
        target: target.filter(|target| source != Some(*target)),
    }
}

pub(crate) struct ChannelsTouched<'a> {
    source: Option<&'a ChannelName>,
    target: Option<&'a ChannelName>,
}

impl<'a> Iterator for ChannelsTouched<'a> {
    type Item = &'a ChannelName;

    fn next(&mut self) -> Option<Self::Item> {
        self.source.take().or_else(|| self.target.take())
    }
}
