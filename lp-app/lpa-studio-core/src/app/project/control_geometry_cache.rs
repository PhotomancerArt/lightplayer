//! The lens's cached geometry for each control product it previews.
//!
//! The control-product probe sends a product's samples every read and its
//! geometry — the sample layout and the display layout, under one revision
//! (`lpc_wire::ControlProductGeometry`) — only when that revision moves.
//! This cache is the client half of that gate. The rules:
//!
//! - **`Changed` replaces** the product's cached geometry.
//! - **`Unchanged` keeps** it — the same `Rc` for the display layout, which
//!   is the lamp renderer's repaint key. An `Unchanged` naming a revision the
//!   cache does not hold cannot stand for anything, and drops it.
//! - **`Omitted` drops** it.
//! - **No cached geometry, no drawing.** Samples that arrive for a product
//!   with nothing cached are not laid out against a guess: the preview keeps
//!   its last good picture, and the next read asks `Always`.
//! - **A refused display layout is cached too** (dome scale: over the link's
//!   byte budget). The samples draw against the cached sample layout with no
//!   lamp positions, and the next read says `IfChanged` with the refusal's
//!   revision — so the engine answers `Unchanged` instead of rebuilding and
//!   re-measuring the same refusal every 150 ms.
//!
//! Dropping the whole cache is explicit ([`ControlGeometryCache::clear`]):
//! `ProjectSync` does it when its claims about the device stop holding.

use std::collections::BTreeMap;
use std::rc::Rc;

use lpc_model::{ControlDisplayLayout, ControlSampleLayout, Revision};
use lpc_wire::{ControlProductGeometry, GeometryProbeResult, GeometryRead};

use crate::UiProductRef;

/// Cached control-product geometry, keyed by the product the lens previews.
#[derive(Debug, Default)]
pub(crate) struct ControlGeometryCache {
    products: BTreeMap<UiProductRef, CachedControlGeometry>,
}

/// One product's geometry, as the lens keeps it between reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CachedControlGeometry {
    revision: Revision,
    /// How the product's samples group into lamps.
    pub(crate) sample_layout: ControlSampleLayout,
    /// Where to draw them — one `Rc` per revision; `None` when the engine
    /// refused the display layout at this revision.
    pub(crate) display_layout: Option<Rc<ControlDisplayLayout>>,
}

impl ControlGeometryCache {
    /// What the next probe for `product` should ask for: `IfChanged` against
    /// the held revision (a refusal included), `Always` when nothing is held.
    pub(crate) fn read_for(&self, product: &UiProductRef) -> GeometryRead {
        match self.products.get(product) {
            Some(geometry) => GeometryRead::IfChanged {
                known_revision: Some(geometry.revision),
            },
            None => GeometryRead::Always,
        }
    }

    /// Fold one probe answer's geometry in, and hand back what the fresh
    /// samples should be drawn against — `None` when nothing current is
    /// held, in which case the samples must not be drawn.
    pub(crate) fn apply(
        &mut self,
        product: UiProductRef,
        answer: &GeometryProbeResult<ControlProductGeometry>,
    ) -> Option<&CachedControlGeometry> {
        match answer {
            GeometryProbeResult::Changed(geometry) => {
                self.products.insert(
                    product,
                    CachedControlGeometry {
                        revision: geometry.revision,
                        sample_layout: geometry.sample_layout.clone(),
                        display_layout: geometry
                            .display_layout
                            .layout()
                            .map(|layout| Rc::new(layout.clone())),
                    },
                );
            }
            GeometryProbeResult::Unchanged { revision } => {
                if self
                    .products
                    .get(&product)
                    .is_some_and(|held| held.revision != *revision)
                {
                    self.products.remove(&product);
                }
            }
            GeometryProbeResult::Omitted => {
                self.products.remove(&product);
            }
        }
        self.products.get(&product)
    }

    /// Drop every product's geometry: the next read asks for all of it.
    pub(crate) fn clear(&mut self) {
        self.products.clear();
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::{
        ColorOrder, ControlExtent, ControlLamp2d, ControlLayout2d, ControlProduct,
        ControlSampleEncoding, ControlSampleSpan, NodeId,
    };
    use lpc_wire::GeometryDisplayLayout;

    use super::*;

    #[test]
    fn nothing_held_asks_always_and_a_changed_answer_is_held() {
        let mut cache = ControlGeometryCache::default();
        assert_eq!(cache.read_for(&product()), GeometryRead::Always);

        let held = cache
            .apply(product(), &changed(12, Some(layout())))
            .expect("a changed answer is drawable");
        assert!(held.display_layout.is_some());
        assert_eq!(
            cache.read_for(&product()),
            GeometryRead::IfChanged {
                known_revision: Some(Revision::new(12)),
            }
        );
    }

    #[test]
    fn unchanged_keeps_the_same_layout_rc() {
        let mut cache = ControlGeometryCache::default();
        let first = cache
            .apply(product(), &changed(12, Some(layout())))
            .and_then(|held| held.display_layout.clone())
            .expect("layout");

        let second = cache
            .apply(
                product(),
                &GeometryProbeResult::Unchanged {
                    revision: Revision::new(12),
                },
            )
            .and_then(|held| held.display_layout.clone())
            .expect("layout");

        assert!(Rc::ptr_eq(&first, &second));
    }

    /// A refused display layout is an answer like any other: held, drawable
    /// (samples with no lamp positions), and not re-asked.
    #[test]
    fn a_refusal_is_held_under_its_revision() {
        let mut cache = ControlGeometryCache::default();
        let held = cache
            .apply(product(), &changed(3, None))
            .expect("the sample layout still draws");
        assert!(held.display_layout.is_none());
        assert_eq!(held.sample_layout.spans.len(), 1);
        assert_eq!(
            cache.read_for(&product()),
            GeometryRead::IfChanged {
                known_revision: Some(Revision::new(3)),
            }
        );
    }

    /// Samples with no current geometry are not drawable, and the next read
    /// asks outright — whether nothing was ever held, the engine omitted it,
    /// or it named a revision the cache does not hold.
    #[test]
    fn no_current_geometry_is_not_drawable_and_asks_again() {
        let mut cache = ControlGeometryCache::default();
        let unheld = GeometryProbeResult::Unchanged {
            revision: Revision::new(12),
        };
        assert!(cache.apply(product(), &unheld).is_none());

        cache.apply(product(), &changed(12, Some(layout())));
        assert!(
            cache
                .apply(
                    product(),
                    &GeometryProbeResult::Unchanged {
                        revision: Revision::new(99),
                    },
                )
                .is_none()
        );
        assert_eq!(cache.read_for(&product()), GeometryRead::Always);

        cache.apply(product(), &changed(12, Some(layout())));
        assert!(
            cache
                .apply(product(), &GeometryProbeResult::Omitted)
                .is_none()
        );
        assert_eq!(cache.read_for(&product()), GeometryRead::Always);

        cache.apply(product(), &changed(12, Some(layout())));
        cache.clear();
        assert_eq!(cache.read_for(&product()), GeometryRead::Always);
    }

    fn product() -> UiProductRef {
        UiProductRef::from_control_product(ControlProduct::new(
            NodeId::new(7),
            0,
            ControlExtent::new(1, 3),
        ))
    }

    fn changed(
        revision: i64,
        layout: Option<ControlDisplayLayout>,
    ) -> GeometryProbeResult<ControlProductGeometry> {
        GeometryProbeResult::Changed(ControlProductGeometry {
            revision: Revision::new(revision),
            sample_layout: ControlSampleLayout {
                spans: vec![ControlSampleSpan {
                    row: 0,
                    start: 0,
                    len: 3,
                    encoding: ControlSampleEncoding::RgbPixels {
                        count: 1,
                        color_order: ColorOrder::Rgb,
                    },
                }],
            },
            display_layout: match layout {
                Some(layout) => GeometryDisplayLayout::Layout(layout),
                None => GeometryDisplayLayout::Unsupported {
                    reason: "over the link's budget".to_string(),
                },
            },
        })
    }

    fn layout() -> ControlDisplayLayout {
        ControlDisplayLayout::Layout2d(ControlLayout2d::new(
            Revision::new(12),
            16,
            16,
            vec![ControlLamp2d {
                lamp_index: 0,
                sample_start: 0,
                center: [0.5, 0.5],
                radius: 0.1,
            }],
        ))
    }
}
