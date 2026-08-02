//! Engine resolution cache: per-frame values over persistent structure.
//!
//! Three tables, all indexed by [`QueryId`], with different lifetimes:
//!
//! - **frame values** — what a query resolved to *this frame*. Discarded each
//!   frame, because producers tick and time moves.
//! - **structural values** — what a query resolves to until the graph changes:
//!   binding literals and authored-definition reads. A def read is a deep copy
//!   of slot data, so re-doing it every frame was one of the larger costs.
//! - **routes** — [`ResolvedRoute`], the decision about *how* a query is
//!   answered. See that type's documentation.
//!
//! The two value tables are behind the `resolver-payload-cache` gate; the
//! route table and the intern table are not. That is the line between the
//! decisions this cache makes and the payloads it holds, and on a part with a
//! 110 KB arena only the decisions are affordable — see
//! [`ResolverCache::set_retain_payloads`].
//!
//! Discarding frame values does not clear the table. Each entry carries the
//! frame it was written for, and a new frame simply stops matching — so a
//! steady scene neither frees nor reallocates its entries, it overwrites them
//! in place. That matters more than it looks: dropping every entry each frame
//! was most of the allocator traffic this cache exists to avoid.

use alloc::rc::Rc;
use alloc::vec::Vec;

use crate::dataflow::resolver::production::{Production, ProductionSource};
use crate::dataflow::resolver::query_intern::QueryId;
use crate::dataflow::resolver::route::ResolvedRoute;

/// Monotonic per-frame stamp. Wrapping is harmless: entries are rewritten far
/// more often than `u32` wraps, and a stale entry would have to survive
/// exactly 2^32 frames to be mistaken for a current one.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
struct FrameStamp(u32);

#[derive(Clone, Debug)]
pub struct ResolverCache {
    frame: FrameStamp,
    values: Vec<Option<(FrameStamp, Production)>>,
    structural: Vec<Option<Production>>,
    routes: Vec<Option<Rc<ResolvedRoute>>>,
    /// Whether [`Self::insert`] stores payloads at all — see
    /// [`Self::set_retain_payloads`].
    retain_payloads: bool,
}

impl Default for ResolverCache {
    fn default() -> Self {
        Self {
            frame: FrameStamp::default(),
            values: Vec::new(),
            structural: Vec::new(),
            routes: Vec::new(),
            retain_payloads: cfg!(feature = "resolver-payload-cache"),
        }
    }
}

impl ResolverCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether resolved *payloads* — the two value tables — are stored at all.
    ///
    /// The route table and the intern table are not affected: those hold
    /// decisions, and decisions are the cheap half of this cache in memory and
    /// the expensive half in cycles. Measured on the classic ESP32
    /// (`projects/test/quad-strips-v3`, 120 LEDs, 110 KB arena):
    ///
    /// | | free heap | fps | `tick` |
    /// |---|---|---|---|
    /// | no cache at all (pre-#243) | 18,128 B | 13 | 69 ms |
    /// | decisions only (`false`) | 18,144 B | 16 | 58 ms |
    /// | decisions + payloads (`true`) | 9,776 B | 21 | 45 ms |
    ///
    /// So the decisions buy 11 ms of the 24 ms for no heap, and the payloads
    /// buy the remaining 13 ms for 8,368 B. On a part with room that is a good
    /// trade and stays on; on a part where 8.3 KB is ~90 LEDs of capacity it is
    /// not, and `fw-esp32v3` leaves the `resolver-payload-cache` gate off.
    ///
    /// Correctness does not depend on the choice — a cache miss recomputes.
    /// `resolution_persistence_tests` asserts both settings render identically.
    pub fn set_retain_payloads(&mut self, retain: bool) {
        self.retain_payloads = retain;
        if !retain {
            // Release whatever a previous setting accumulated, capacity and
            // all: a table that is never read again must not hold pages.
            self.values = Vec::new();
            self.structural = Vec::new();
        }
    }

    /// Whether a production may outlive the frame that computed it.
    ///
    /// A binding literal and an authored-def read are both functions of the
    /// project, not of the frame — nothing but a structural change can move
    /// them. Everything else either ticks (producers) or is built from things
    /// that tick (merges, values reached through a binding).
    pub fn is_structural(source: &ProductionSource) -> bool {
        matches!(
            source,
            ProductionSource::Literal | ProductionSource::Default
        )
    }

    /// Begin a frame: this frame's values start empty, structure survives.
    pub fn begin_frame(&mut self) {
        self.frame = FrameStamp(self.frame.0.wrapping_add(1));
    }

    /// The graph changed shape: everything cached is void.
    pub fn invalidate_structure(&mut self) {
        self.values.clear();
        self.structural.clear();
        self.routes.clear();
        // Ids are reassigned from zero after an epoch change, so the tables
        // must not keep entries addressed by the old numbering.
    }

    pub fn get(&self, id: QueryId) -> Option<&Production> {
        if let Some(Some((stamp, production))) = self.values.get(id.index()) {
            if *stamp == self.frame {
                return Some(production);
            }
        }
        self.structural.get(id.index()).and_then(Option::as_ref)
    }

    pub fn insert(&mut self, id: QueryId, production: Production) {
        if !self.retain_payloads {
            return;
        }
        if Self::is_structural(&production.source) {
            grow_to(&mut self.structural, id.index());
            self.structural[id.index()] = Some(production);
            return;
        }
        grow_to(&mut self.values, id.index());
        self.values[id.index()] = Some((self.frame, production));
    }

    /// Routes are shared rather than copied: a cache hit on the hot path must
    /// not deep-copy the binding sources it just avoided recomputing.
    pub fn route(&self, id: QueryId) -> Option<&Rc<ResolvedRoute>> {
        self.routes.get(id.index()).and_then(Option::as_ref)
    }

    pub fn insert_route(&mut self, id: QueryId, route: Rc<ResolvedRoute>) {
        grow_to(&mut self.routes, id.index());
        self.routes[id.index()] = Some(route);
    }

    /// Whether anything is cached for the current frame. Used by tests that
    /// assert a fresh resolver starts empty.
    pub fn is_empty(&self) -> bool {
        self.structural.iter().all(Option::is_none)
            && self
                .values
                .iter()
                .all(|slot| !matches!(slot, Some((stamp, _)) if *stamp == self.frame))
    }
}

fn grow_to<T>(table: &mut Vec<Option<T>>, index: usize) {
    if table.len() <= index {
        table.resize_with(index + 1, || None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dataflow::binding::BindingRef;
    use crate::dataflow::resolver::query_intern::QueryInternTable;
    use crate::dataflow::resolver::query_key::QueryKey;
    use alloc::string::String;
    use lpc_model::ChannelName;
    use lpc_model::NodeId;
    use lpc_model::Revision;
    use lpc_model::SlotPath;
    use lpc_model::WithRevision;
    use lps_shared::LpsValueF32;

    fn produced(value: f32, source: ProductionSource) -> Production {
        Production::value(
            WithRevision::new(Revision::new(1), LpsValueF32::F32(value)),
            source,
        )
        .expect("scalar production")
    }

    fn producer_source() -> ProductionSource {
        ProductionSource::ProducedSlot {
            node: NodeId::new(0),
            slot: SlotPath::parse("out").expect("path"),
        }
    }

    fn id(table: &mut QueryInternTable, name: &str) -> QueryId {
        table.intern(&QueryKey::Bus(ChannelName(String::from(name))))
    }

    #[test]
    fn frame_values_do_not_survive_the_frame() {
        let mut table = QueryInternTable::new();
        let mut cache = ResolverCache::new();
        let key = id(&mut table, "video");

        cache.insert(key, produced(1.0, producer_source()));
        assert!(cache.get(key).is_some());

        cache.begin_frame();
        assert!(
            cache.get(key).is_none(),
            "a producer's value belongs to the frame that produced it"
        );
    }

    #[test]
    fn structural_values_survive_frames_but_not_invalidation() {
        let mut table = QueryInternTable::new();
        let mut cache = ResolverCache::new();
        let key = id(&mut table, "literal");

        cache.insert(key, produced(2.0, ProductionSource::Literal));
        cache.begin_frame();
        cache.begin_frame();
        assert!(
            cache.get(key).is_some(),
            "a binding literal cannot change without a structural change"
        );

        cache.invalidate_structure();
        assert!(cache.get(key).is_none());
    }

    #[test]
    fn authored_def_reads_are_structural() {
        assert!(ResolverCache::is_structural(&ProductionSource::Default));
        assert!(ResolverCache::is_structural(&ProductionSource::Literal));
        assert!(!ResolverCache::is_structural(&ProductionSource::Merged));
        assert!(!ResolverCache::is_structural(&producer_source()));
        assert!(!ResolverCache::is_structural(
            &ProductionSource::BusBinding {
                binding: BindingRef::new(NodeId::new(0), 0),
            }
        ));
    }

    #[test]
    fn routes_survive_frames_but_not_invalidation() {
        let mut table = QueryInternTable::new();
        let mut cache = ResolverCache::new();
        let key = id(&mut table, "routed");

        cache.insert_route(key, Rc::new(ResolvedRoute::Produce));
        cache.begin_frame();
        assert!(
            matches!(cache.route(key).map(|r| &**r), Some(ResolvedRoute::Produce)),
            "which binding answers a query changes only when bindings change"
        );

        cache.invalidate_structure();
        assert!(cache.route(key).is_none());
    }

    #[test]
    fn a_later_frame_overwrites_rather_than_reallocating() {
        let mut table = QueryInternTable::new();
        let mut cache = ResolverCache::new();
        let key = id(&mut table, "video");

        cache.insert(key, produced(1.0, producer_source()));
        cache.begin_frame();
        cache.insert(key, produced(3.0, producer_source()));

        let value = cache.get(key).expect("current frame value");
        assert!(value.as_value().expect("value").eq(&LpsValueF32::F32(3.0)));
        assert_eq!(cache.values.len(), 1, "the table is reused, not regrown");
    }

    #[test]
    fn sparse_ids_do_not_read_neighbouring_entries() {
        let mut cache = ResolverCache::new();
        let mut table = QueryInternTable::new();
        let low = id(&mut table, "a");
        let _skipped = id(&mut table, "b");
        let high = id(&mut table, "c");

        cache.insert(high, produced(5.0, producer_source()));
        assert!(cache.get(low).is_none());
        assert!(cache.get(high).is_some());
    }
}
