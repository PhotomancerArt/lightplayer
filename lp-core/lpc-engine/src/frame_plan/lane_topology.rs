//! The engine's lane plan: which output is the default for its bus channel.

use lp_collection::VecMap;
use lpc_model::NodeId;

/// Per output (any node consuming `input` from a bus channel), whether it is
/// the DEFAULT fragments consumer of that channel — the target of every
/// unaddressed patch run and of auto-flow (D40). Derived from the binding
/// graph and the consumers' authored merge policies, so it moves only with a
/// structural edit: the engine keeps it in a [`super::PlanSlot`] keyed on the
/// resolver's structure epoch, built for every node at once, and every
/// output's per-frame placement question reads it.
///
/// It replaced the fps spike's lazily filled per-node cache (cleared by the
/// tree on each binding-index edit), so the answer has the frame plans'
/// checker behind it instead of an invalidation list of its own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct LaneTopology {
    default_fragments_consumer: VecMap<NodeId, bool>,
}

impl LaneTopology {
    /// Ask `is_default` about every node in `nodes`.
    pub(crate) fn build(
        nodes: impl Iterator<Item = NodeId>,
        mut is_default: impl FnMut(NodeId) -> bool,
    ) -> Self {
        let mut default_fragments_consumer = VecMap::new();
        for node in nodes {
            default_fragments_consumer.insert(node, is_default(node));
        }
        Self {
            default_fragments_consumer,
        }
    }

    /// The answer for `consumer`, or `None` for a node the plan was not built
    /// over (attached since — the caller derives it fresh).
    pub(crate) fn is_default_fragments_consumer(&self, consumer: NodeId) -> Option<bool> {
        self.default_fragments_consumer.get(&consumer).copied()
    }
}
