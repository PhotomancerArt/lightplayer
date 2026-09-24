//! When each probed control product's sample layout last changed.
//!
//! The control-product probe gates its geometry (sample layout + display
//! layout) behind one revision. The display layout carries its own revision
//! (a fixture stamps it when its mapping or render size moves), but a sample
//! layout is just a value the render hands back — a fixture's color order or
//! diagnostic mode regroups the samples without touching the display layout's
//! revision. So the engine remembers the last sample layout it answered for
//! each product and stamps the revision at which it last CHANGED, compared by
//! value: nothing a node forgets to declare can slip past it.
//!
//! The stamp is the engine's frame revision at the probe that saw the change.
//! A control product's sample layout is a function of state its node latched
//! during a tick, so two probes in one frame revision see the same layout;
//! a change therefore always lands on a revision later than any stamp handed
//! out before it, which is what makes `max(stamp, display revision)` an
//! honest geometry revision.

use alloc::vec::Vec;

use lpc_model::{ControlProduct, NodeId, Revision};

use crate::products::control::ControlLayout;

/// Per-product sample-layout change stamps.
///
/// Keyed by the whole [`ControlProduct`] (node, output AND extent): two
/// clients asking one node at different extents get differently grouped
/// samples, and must not overwrite each other's stamp into a false change.
#[derive(Debug, Default)]
pub(crate) struct ControlGeometryStamps {
    entries: Vec<ControlGeometryStamp>,
}

#[derive(Debug)]
struct ControlGeometryStamp {
    product: ControlProduct,
    sample_layout: ControlLayout,
    changed_at: Revision,
}

impl ControlGeometryStamps {
    /// The revision at which `product`'s sample layout last changed, given
    /// the layout this probe rendered at frame revision `now`.
    ///
    /// Entries for nodes that are no longer `alive` are dropped on the way,
    /// so the store holds at most one layout per probed live product.
    pub(crate) fn stamp(
        &mut self,
        product: ControlProduct,
        sample_layout: &ControlLayout,
        now: Revision,
        alive: impl Fn(NodeId) -> bool,
    ) -> Revision {
        self.entries.retain(|entry| alive(entry.product.node()));
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.product == product)
        {
            Some(entry) if entry.sample_layout == *sample_layout => entry.changed_at,
            Some(entry) => {
                entry.sample_layout = sample_layout.clone();
                entry.changed_at = now;
                now
            }
            None => {
                self.entries.push(ControlGeometryStamp {
                    product,
                    sample_layout: sample_layout.clone(),
                    changed_at: now,
                });
                now
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use lpc_model::{ColorOrder, ControlExtent, ControlSampleEncoding, ControlSampleSpan};

    use super::*;

    #[test]
    fn an_unchanged_layout_keeps_its_first_stamp() {
        let mut stamps = ControlGeometryStamps::default();
        let product = product(3);
        let layout = layout(ColorOrder::Rgb);
        assert_eq!(
            stamps.stamp(product, &layout, Revision::new(5), |_| true),
            Revision::new(5)
        );
        assert_eq!(
            stamps.stamp(product, &layout, Revision::new(9), |_| true),
            Revision::new(5),
            "the same layout at a later frame is not a change"
        );
    }

    #[test]
    fn a_regrouped_layout_restamps_at_the_frame_that_saw_it() {
        let mut stamps = ControlGeometryStamps::default();
        let product = product(3);
        stamps.stamp(product, &layout(ColorOrder::Rgb), Revision::new(5), |_| {
            true
        });
        assert_eq!(
            stamps.stamp(product, &layout(ColorOrder::Grb), Revision::new(8), |_| {
                true
            }),
            Revision::new(8)
        );
    }

    #[test]
    fn dead_nodes_leave_the_store() {
        let mut stamps = ControlGeometryStamps::default();
        stamps.stamp(
            product(3),
            &layout(ColorOrder::Rgb),
            Revision::new(5),
            |_| true,
        );
        stamps.stamp(
            product(4),
            &layout(ColorOrder::Rgb),
            Revision::new(6),
            |node| node != NodeId::new(3),
        );
        assert_eq!(stamps.entries.len(), 1);
        assert_eq!(stamps.entries[0].product.node(), NodeId::new(4));
    }

    fn product(node: u32) -> ControlProduct {
        ControlProduct::new(NodeId::new(node), 0, ControlExtent::new(1, 3))
    }

    fn layout(color_order: ColorOrder) -> ControlLayout {
        ControlLayout {
            spans: vec![ControlSampleSpan {
                row: 0,
                start: 0,
                len: 3,
                encoding: ControlSampleEncoding::RgbPixels {
                    count: 1,
                    color_order,
                },
            }],
        }
    }
}
