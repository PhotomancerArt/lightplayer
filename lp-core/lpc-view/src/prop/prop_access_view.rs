//! Object-safe read-only view of properties held in view cache / wire state.

use alloc::boxed::Box;
use alloc::vec::Vec;

use lpc_model::ModelValue;
use lpc_model::project::FrameId;
use lpc_model::prop::PropPath;

/// Reflection over cached wire-safe property values held by a view/cache.
pub trait PropAccessView {
    /// Get the current value at `path`, if any.
    fn get(&self, path: &PropPath) -> Option<(&ModelValue, FrameId)>;

    /// Iterate entries whose `changed_frame > since`.
    fn iter_changed_since<'a>(
        &'a self,
        since: FrameId,
    ) -> Box<dyn Iterator<Item = (&'a PropPath, &'a ModelValue, FrameId)> + 'a>;

    /// All cached entries.
    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a PropPath, &'a ModelValue, FrameId)> + 'a>;
}

/// Simple in-memory cache backing [`PropAccessView`] for tests and small clients.
#[derive(Default)]
pub struct PropsMapView {
    values: Vec<(PropPath, ModelValue, FrameId)>,
}

impl PropsMapView {
    pub fn insert(&mut self, path: PropPath, value: ModelValue, frame: FrameId) {
        if let Some(i) = self.values.iter().position(|(p, _, _)| p == &path) {
            self.values[i] = (path, value, frame);
        } else {
            self.values.push((path, value, frame));
        }
    }
}

impl PropAccessView for PropsMapView {
    fn get(&self, path: &PropPath) -> Option<(&ModelValue, FrameId)> {
        self.values
            .iter()
            .find(|(p, _, _)| p == path)
            .map(|(_, v, f)| (v, *f))
    }

    fn iter_changed_since<'a>(
        &'a self,
        since: FrameId,
    ) -> Box<dyn Iterator<Item = (&'a PropPath, &'a ModelValue, FrameId)> + 'a> {
        Box::new(
            self.values
                .iter()
                .filter(move |(_, _, frame)| frame.as_i64() > since.as_i64())
                .map(|(p, v, f)| (p, v, *f)),
        )
    }

    fn snapshot<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a PropPath, &'a ModelValue, FrameId)> + 'a> {
        Box::new(self.values.iter().map(|(p, v, f)| (p, v, *f)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::prop::prop_path::parse_path;

    #[test]
    fn prop_access_view_is_object_safe() {
        let _: Box<dyn PropAccessView> = Box::new(PropsMapView::default());
    }

    #[test]
    fn map_get_and_snapshot() {
        let mut m = PropsMapView::default();
        let p = parse_path("outputs.x").unwrap();
        m.insert(p.clone(), ModelValue::F32(2.0), FrameId::new(1));

        assert_eq!(m.get(&p), Some((&ModelValue::F32(2.0), FrameId::new(1))));
        assert_eq!(m.snapshot().count(), 1);
    }

    #[test]
    fn iter_changed_since_respects_frame() {
        let mut m = PropsMapView::default();
        let p1 = parse_path("a").unwrap();
        let p2 = parse_path("b").unwrap();
        m.insert(p1.clone(), ModelValue::I32(1), FrameId::new(1));
        m.insert(p2.clone(), ModelValue::I32(2), FrameId::new(10));

        let recent: Vec<_> = m.iter_changed_since(FrameId::new(5)).collect();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].0, &p2);
    }
}
