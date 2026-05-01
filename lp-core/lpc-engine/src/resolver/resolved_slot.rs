//! ResolvedSlot — one entry in the per-node **slot** resolver cache (legacy /
//! transitional data path storing [`lps_shared::LpsValueF32`] directly).
//!
//! The M2 demand-driven engine path caches [`crate::resolver::production::Production`]
//! (versioned [`crate::runtime_product::RuntimeProduct`]) instead. Do not assume
//! every resolved runtime value is representable only as `LpsValueF32`.

use crate::resolver::resolve_source::ResolveSource;
use lpc_model::FrameId;
use lps_shared::LpsValueF32;

/// One entry in the per-node **slot** resolver cache.
///
/// `value` is shader-runtime [`LpsValueF32`] from this cascade; `changed_frame`
/// is the frame at which this cached value last differed from its previous
/// resolution; `source` records provenance.
///
/// This remains the slot-/binding-resolution shape. It is **not** the M2
/// `Production` cache used by `ResolveSession`.
///
/// Constructed by the M4.3 resolver; M4.2 only ships the data
/// shape so `NodeEntry`'s commented `prop_cache` stub can resolve
/// to a real type name.
#[derive(Clone, Debug)]
pub struct ResolvedSlot {
    pub value: LpsValueF32,
    pub changed_frame: FrameId,
    pub source: ResolveSource,
}

impl ResolvedSlot {
    pub fn new(value: LpsValueF32, changed_frame: FrameId, source: ResolveSource) -> Self {
        Self {
            value,
            changed_frame,
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameId, ResolveSource, ResolvedSlot};
    use lps_shared::LpsValueF32;

    #[test]
    fn resolved_slot_construct() {
        let slot = ResolvedSlot::new(
            LpsValueF32::F32(1.5),
            FrameId::new(10),
            ResolveSource::Default,
        );
        assert!(matches!(slot.source, ResolveSource::Default));
        assert_eq!(slot.changed_frame.as_i64(), 10);
    }

    #[test]
    fn resolved_slot_clone() {
        let slot = ResolvedSlot::new(
            LpsValueF32::F32(2.0),
            FrameId::new(5),
            ResolveSource::Failed,
        );
        let cloned = slot.clone();
        assert!(matches!(cloned.source, ResolveSource::Failed));
        assert_eq!(cloned.changed_frame.as_i64(), 5);
    }

    #[test]
    fn resolved_slot_debug_prints() {
        let slot = ResolvedSlot::new(
            LpsValueF32::F32(3.0),
            FrameId::new(1),
            ResolveSource::Default,
        );
        let s = alloc::format!("{slot:?}");
        assert!(s.contains("ResolvedSlot"));
        assert!(s.contains("F32(3.0)"));
    }
}
