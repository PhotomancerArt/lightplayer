//! Resolver — demand-resolution cache owner.
//!
//! # Invalidation contract
//!
//! The resolver distinguishes two kinds of "this is no longer valid", and
//! every caller must pick the right one:
//!
//! - [`Resolver::begin_frame`] — a new frame starts. Values resolved *for the
//!   previous frame* are stale, because producers tick again and time moves.
//!   The shape of the graph has not changed.
//! - [`Resolver::invalidate_structure`] — the graph itself changed: bindings,
//!   tree topology, node definitions, or project state. Everything the
//!   resolver knows is suspect, including which binding wins a slot.
//!
//! **Any future engine mutation that can change what a query resolves
//! *through* — as opposed to what it resolves *to* this frame — must call
//! [`Resolver::invalidate_structure`].** Getting this wrong does not fail
//! loudly; it serves a stale answer. The known sites are enumerated in
//! `docs/adr/2026-07-31-resolver-persistent-resolution.md`.
//!
//! Frame-scoped and structure-scoped invalidation are deliberately separate
//! names rather than one `clear()`, so that persisting resolution across
//! frames is a change of what each one *does*, not a hunt for which callers
//! meant which.

use crate::dataflow::resolver::query_intern::{QueryId, QueryInternTable};
use crate::dataflow::resolver::query_key::QueryKey;
use crate::dataflow::resolver::resolve_error::ResolveError;
use crate::dataflow::resolver::resolver_cache::ResolverCache;
use lp_collection::VecMap;
use lpc_model::LpValue;
use lpc_model::NodeId;
use lpc_model::Revision;
use lpc_model::SlotPath;
use lpc_model::SlotPathError;
use lpc_model::WithRevision;
use lps_shared::LpsValueF32;

/// Resolution work performed during one frame.
///
/// These exist to make "did the frame re-resolve the graph from cold?" a
/// testable question rather than a profiling exercise: a steady-state frame
/// over an unchanged graph should perform no structural work at all. They are
/// always on — each is a single increment on a path that already allocates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResolveFrameCounters {
    /// Binding-index lookups (`binding_for_consumed_slot`,
    /// `bindings_for_consumed_slot`, `providers_for_bus`).
    pub binding_lookups: u32,
    /// Merge-policy reads, each of which introspects an authored node def.
    pub merge_policy_reads: u32,
    /// Productions answered from an authored def (`ProductionSource::Default`).
    pub def_produces: u32,
    /// Binding literals materialized into productions.
    pub literal_materializations: u32,
    /// Productions answered by ticking a producer node.
    pub producer_produces: u32,
    /// Queries answered from cache.
    pub cache_hits: u32,
    /// Queries that had to be resolved.
    pub uncached_resolves: u32,
}

impl ResolveFrameCounters {
    /// Work that only a structural change can invalidate.
    ///
    /// A steady-state frame over an unchanged graph should report zero here;
    /// anything else means the frame re-derived the graph it already knew.
    pub fn structural_work(&self) -> u32 {
        self.binding_lookups + self.merge_policy_reads + self.def_produces
    }
}

/// Owns the [`ResolverCache`] and the query intern table.
#[derive(Clone, Debug, Default)]
pub struct Resolver {
    cache: ResolverCache,
    intern: QueryInternTable,
    structure_epoch: u64,
    frame_counters: ResolveFrameCounters,
    force_invalidate_per_frame: bool,
    /// `(node, authored path)` → the consumed-slot query it names.
    ///
    /// Nodes read a handful of authored slots by constant path every frame
    /// (`power.some`, `diagnostic_mode`, each shader input). Parsing those
    /// strings per frame allocated a `SlotPath` per read, only to drop it
    /// again — one of the last per-frame allocation sources after routes and
    /// values stopped being rebuilt. Lives here because it shares the intern
    /// table's lifetime exactly.
    static_paths: VecMap<(NodeId, &'static str), QueryId>,
}

impl Resolver {
    pub fn new() -> Self {
        Self {
            cache: ResolverCache::new(),
            intern: QueryInternTable::new(),
            structure_epoch: 0,
            frame_counters: ResolveFrameCounters::default(),
            force_invalidate_per_frame: false,
            static_paths: VecMap::new(),
        }
    }

    pub fn cache(&self) -> &ResolverCache {
        &self.cache
    }

    pub fn cache_mut(&mut self) -> &mut ResolverCache {
        &mut self.cache
    }

    pub fn intern(&self) -> &QueryInternTable {
        &self.intern
    }

    /// Id for `query`, assigning one on first use this epoch.
    pub fn intern_query(&mut self, query: &QueryKey) -> QueryId {
        self.intern.intern(query)
    }

    /// Id for `node`'s consumed slot at the compile-time-constant `path`,
    /// parsing the path at most once per epoch.
    pub fn intern_static_consumed(
        &mut self,
        node: NodeId,
        path: &'static str,
    ) -> Result<QueryId, SlotPathError> {
        if let Some(id) = self.static_paths.get(&(node, path)) {
            return Ok(*id);
        }
        let slot = SlotPath::parse(path)?;
        let id = self.intern.intern(&QueryKey::ConsumedSlot { node, slot });
        self.static_paths.insert((node, path), id);
        Ok(id)
    }

    /// Start a new frame: per-frame resolved values are discarded.
    pub fn begin_frame(&mut self) {
        self.frame_counters = ResolveFrameCounters::default();
        if self.force_invalidate_per_frame {
            self.invalidate_structure();
            return;
        }
        self.cache.begin_frame();
    }

    /// The graph changed shape: every cached decision and value is suspect.
    pub fn invalidate_structure(&mut self) {
        self.structure_epoch = self.structure_epoch.wrapping_add(1);
        self.cache.invalidate_structure();
        self.intern.clear();
        self.static_paths.clear();
    }

    /// Throw away *all* resolution every frame, as if the graph changed shape
    /// each tick — the behaviour that predates cross-frame persistence.
    ///
    /// This exists so a test can run the same scene twice, cached and
    /// uncached, and demand identical output. That differential is the only
    /// check that reliably catches a stale cache: a wrong cached answer is
    /// still a *plausible* answer, so it survives assertions written against
    /// what the code does rather than against what it should do.
    pub fn set_force_invalidate_per_frame(&mut self, force: bool) {
        self.force_invalidate_per_frame = force;
    }

    /// Keep the route and intern caches, drop the value caches — see
    /// [`ResolverCache::set_retain_payloads`] for the measured trade.
    ///
    /// Unlike [`Self::set_force_invalidate_per_frame`] this is a supported
    /// production setting, not a test lever: `fw-esp32v3` runs this way,
    /// because on a 110 KB arena the payload tables cost more LEDs than they
    /// buy frames.
    pub fn set_retain_payloads(&mut self, retain: bool) {
        self.cache.set_retain_payloads(retain);
    }

    /// How many times the graph has changed shape. Only equality across two
    /// observations is meaningful; the absolute value is not.
    pub fn structure_epoch(&self) -> u64 {
        self.structure_epoch
    }

    /// Resolution work done for the current frame. Counting resets when the
    /// next frame begins, so after a tick returns this reads as "what that
    /// tick cost".
    pub fn frame_counters(&self) -> &ResolveFrameCounters {
        &self.frame_counters
    }

    pub(crate) fn counters_mut(&mut self) -> &mut ResolveFrameCounters {
        &mut self.frame_counters
    }
}

/// Materialize a direct [`lpc_model::LpValue`] literal to a versioned slot value.
pub(crate) fn materialize_literal_product(
    value: &lpc_model::LpValue,
    frame: Revision,
) -> WithRevision<LpValue> {
    WithRevision::new(frame, value.clone())
}

/// Convert [`lpc_model::LpValue`] to [`LpsValueF32`] for shader-compatible literals.
///
/// Not every runtime domain maps 1:1 into `LpsValueF32`; products and resources
/// remain [`LpValue`] leaves until a node explicitly consumes their domain.
pub(crate) fn model_value_to_lps_value_f32(
    value: &lpc_model::LpValue,
) -> Result<LpsValueF32, ResolveError> {
    use lpc_model::LpValue;

    match value {
        LpValue::I32(v) => Ok(LpsValueF32::I32(*v)),
        LpValue::U32(v) => Ok(LpsValueF32::U32(*v)),
        LpValue::F32(v) => Ok(LpsValueF32::F32(*v)),
        LpValue::Bool(v) => Ok(LpsValueF32::Bool(*v)),
        LpValue::Vec2(v) => Ok(LpsValueF32::Vec2(*v)),
        LpValue::Vec3(v) => Ok(LpsValueF32::Vec3(*v)),
        LpValue::Vec4(v) => Ok(LpsValueF32::Vec4(*v)),
        LpValue::IVec2(v) => Ok(LpsValueF32::IVec2(*v)),
        LpValue::IVec3(v) => Ok(LpsValueF32::IVec3(*v)),
        LpValue::IVec4(v) => Ok(LpsValueF32::IVec4(*v)),
        LpValue::UVec2(v) => Ok(LpsValueF32::UVec2(*v)),
        LpValue::UVec3(v) => Ok(LpsValueF32::UVec3(*v)),
        LpValue::UVec4(v) => Ok(LpsValueF32::UVec4(*v)),
        LpValue::BVec2(v) => Ok(LpsValueF32::BVec2(*v)),
        LpValue::BVec3(v) => Ok(LpsValueF32::BVec3(*v)),
        LpValue::BVec4(v) => Ok(LpsValueF32::BVec4(*v)),
        LpValue::Mat2x2(v) => Ok(LpsValueF32::Mat2x2(*v)),
        LpValue::Mat3x3(v) => Ok(LpsValueF32::Mat3x3(*v)),
        LpValue::Mat4x4(v) => Ok(LpsValueF32::Mat4x4(*v)),
        LpValue::Array(items) => {
            let mut result = alloc::vec::Vec::with_capacity(items.len());
            for item in items.iter() {
                result.push(model_value_to_lps_value_f32(item)?);
            }
            Ok(LpsValueF32::Array(result.into_boxed_slice()))
        }
        LpValue::Struct { name, fields } => {
            let mut result_fields = alloc::vec::Vec::with_capacity(fields.len());
            for (k, v) in fields.iter() {
                result_fields.push((k.clone(), model_value_to_lps_value_f32(v)?));
            }
            Ok(LpsValueF32::Struct {
                name: name.clone(),
                fields: result_fields,
            })
        }
        LpValue::Buffer(buffer) => Ok(LpsValueF32::Buffer(
            lps_shared::LpsBuffer::from_words(
                crate::shader_abi::model_buffer_elem_to_lps(buffer.elem),
                buffer.words().to_vec().into_boxed_slice(),
            )
            .map_err(|e| ResolveError::new(alloc::format!("buffer value: {e}")))?,
        )),
        LpValue::Unset
        | LpValue::String(_)
        | LpValue::Enum { .. }
        | LpValue::Resource(_)
        | LpValue::Product(_) => Err(ResolveError::new(alloc::format!(
            "model value cannot be resolved as shader value: {value:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use lpc_model::LpValue;

    #[test]
    fn model_value_conversion_f32() {
        let val = LpValue::F32(3.14);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        assert!(matches!(lps, LpsValueF32::F32(3.14)));
    }

    #[test]
    fn model_value_conversion_vec3() {
        let val = LpValue::Vec3([1.0, 2.0, 3.0]);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        assert!(matches!(lps, LpsValueF32::Vec3([1.0, 2.0, 3.0])));
    }

    #[test]
    fn model_value_conversion_array() {
        let val = LpValue::Array(alloc::vec![LpValue::F32(1.0), LpValue::F32(2.0),]);
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        match lps {
            LpsValueF32::Array(items) => {
                assert_eq!(items.len(), 2);
                assert!(matches!(items[0], LpsValueF32::F32(1.0)));
                assert!(matches!(items[1], LpsValueF32::F32(2.0)));
            }
            _ => panic!("expected array"),
        }
    }

    #[test]
    fn model_value_conversion_struct() {
        let val = LpValue::Struct {
            name: Some(String::from("Test")),
            fields: alloc::vec![
                (String::from("x"), LpValue::F32(1.0)),
                (String::from("y"), LpValue::F32(2.0)),
            ],
        };
        let lps = model_value_to_lps_value_f32(&val).unwrap();
        match lps {
            LpsValueF32::Struct { name, fields } => {
                assert_eq!(name.as_deref(), Some("Test"));
                assert_eq!(fields.len(), 2);
            }
            _ => panic!("expected struct"),
        }
    }
}
