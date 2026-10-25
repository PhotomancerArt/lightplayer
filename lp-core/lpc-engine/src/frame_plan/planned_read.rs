//! A read a node makes every frame, planned to its interned query id.

use crate::dataflow::resolver::QueryId;

/// One per-frame read site's interned query, kept by the node that reads it.
///
/// A node's settings reads (a fixture's `render_size`, `brightness`, its
/// `consume` policy; an output's `test_pattern`) used to build or look up
/// their query key on every read: a clone of the accessor, or a search of
/// the resolver's memo by path. The id is the same all epoch long, so the
/// node keeps it here — tagged with the structure epoch it was interned in,
/// because ids are epoch-scoped — and every later read that epoch resolves
/// by id ([`crate::node::TickContext::resolve_planned_static`] and its
/// siblings). The answer itself is still resolved per frame: a read
/// through a bus writer stays live, and a default or literal is a cache hit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlannedRead {
    planned: Option<(u64, QueryId)>,
}

impl PlannedRead {
    pub const fn new() -> Self {
        Self { planned: None }
    }

    /// The id, if it was interned in `epoch`.
    pub(crate) fn id_for(&self, epoch: u64) -> Option<QueryId> {
        match self.planned {
            Some((held, id)) if held == epoch => Some(id),
            _ => None,
        }
    }

    pub(crate) fn plan(&mut self, epoch: u64, id: QueryId) {
        self.planned = Some((epoch, id));
    }
}
