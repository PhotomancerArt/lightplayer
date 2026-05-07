//! Legacy object-safe read-only view of flat properties held in view cache / wire state.
//!
//! This predates slot shapes and cannot represent container versions or
//! structural metadata. New UI code should use [`crate::SlotMirrorView`].

use alloc::boxed::Box;
use alloc::vec::Vec;

use lpc_model::LpValue;
use lpc_model::project::FrameId;
use lpc_model::value::ValuePath;

/// Reflection over cached wire-safe property values held by a view/cache.
pub trait PropAccessView {
    /// Get the current value at `path`, if any.
    fn get(&self, path: &ValuePath) -> Option<(&LpValue, FrameId)>;

    /// Iterate entries whose `changed_frame > since`.
    fn iter_changed_since<'a>(
        &'a self,
        since: FrameId,
    ) -> Box<dyn Iterator<Item = (&'a ValuePath, &'a LpValue, FrameId)> + 'a>;

    /// All cached entries.
    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a ValuePath, &'a LpValue, FrameId)> + 'a>;
}

/// Simple in-memory cache backing [`PropAccessView`] for tests and small clients.
#[derive(Default)]
pub struct PropsMapView {
    values: Vec<(ValuePath, LpValue, FrameId)>,
}

impl PropsMapView {
    pub fn insert(&mut self, path: ValuePath, value: LpValue, frame: FrameId) {
        if let Some(i) = self.values.iter().position(|(p, _, _)| p == &path) {
            self.values[i] = (path, value, frame);
        } else {
            self.values.push((path, value, frame));
        }
    }
}

impl PropAccessView for PropsMapView {
    fn get(&self, path: &ValuePath) -> Option<(&LpValue, FrameId)> {
        self.values
            .iter()
            .find(|(p, _, _)| p == path)
            .map(|(_, v, f)| (v, *f))
    }

    fn iter_changed_since<'a>(
        &'a self,
        since: FrameId,
    ) -> Box<dyn Iterator<Item = (&'a ValuePath, &'a LpValue, FrameId)> + 'a> {
        Box::new(
            self.values
                .iter()
                .filter(move |(_, _, frame)| frame.as_i64() > since.as_i64())
                .map(|(p, v, f)| (p, v, *f)),
        )
    }

    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a ValuePath, &'a LpValue, FrameId)> + 'a> {
        Box::new(self.values.iter().map(|(p, v, f)| (p, v, *f)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::value::value_path::parse_path;

    #[test]
    fn prop_access_view_is_object_safe() {
        let _: Box<dyn PropAccessView> = Box::new(PropsMapView::default());
    }

    #[test]
    fn map_get_and_snapshot() {
        let mut m = PropsMapView::default();
        let p = parse_path("outputs.x").unwrap();
        m.insert(p.clone(), LpValue::F32(2.0), FrameId::new(1));

        assert_eq!(m.get(&p), Some((&LpValue::F32(2.0), FrameId::new(1))));
        assert_eq!(m.snapshot().count(), 1);
    }

    #[test]
    fn iter_changed_since_respects_frame() {
        let mut m = PropsMapView::default();
        let p1 = parse_path("a").unwrap();
        let p2 = parse_path("b").unwrap();
        m.insert(p1.clone(), LpValue::I32(1), FrameId::new(1));
        m.insert(p2.clone(), LpValue::I32(2), FrameId::new(10));

        let recent: Vec<_> = m.iter_changed_since(FrameId::new(5)).collect();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].0, &p2);
    }
}
