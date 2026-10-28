//! The output's frame plan: where each producer's samples land, planned once
//! per edit.

use alloc::vec::Vec;

use lpc_model::{NodeRuntimeStatus, Revision};

use super::output_node::{
    FragmentCoverage, FragmentPlacement, OutputFragment, coverage_status, fragment_coverage,
    plan_fragments,
};
use crate::node::TickContext;
use crate::products::control::ControlProduct;

/// What an output used to re-derive every frame from its producers: the
/// fragments planned from their placements (each producer's patch as this
/// output sees it: the runs naming it), and what those
/// fragments cover. Built by [`Self::build`] the first frame after its key moves and
/// replayed after; only the render, the reversal and rotation, the overlays
/// and the publish stay per frame.
///
/// Held in a [`crate::frame_plan::PlanSlot`] keyed on [`OutputPlanKey`].
/// The producers' products are part of what the plan answers for: a frame
/// whose resolved products differ from [`Self::products`] (a fixture's extent
/// moved) rebuilds it, as a key change would.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OutputFramePlan {
    /// The control products this plan was built for, in fragment order.
    pub(crate) products: Vec<ControlProduct>,
    pub(crate) fragments: Vec<OutputFragment>,
    pub(crate) coverage: FragmentCoverage,
    /// The placement complaint for this fragment set, if any.
    pub(crate) status: Option<NodeRuntimeStatus>,
}

/// What can move an output's plan: a structural edit (bindings, defs,
/// patches, mappings and render sizes all move the epoch), and the
/// registered output-name set (a patch run names its output; a name
/// registered on a later frame of a load re-routes it).
///
/// And the producers' patch generations, summed: a fixture can re-resolve its
/// patch on a frame where nothing structural moved (the first frame it sees
/// a sibling output's name), and its generation only ever counts up, so any
/// change moves the sum.
pub(crate) type OutputPlanKey = (u64, Revision, u64);

impl OutputFramePlan {
    pub(crate) fn key(ctx: &TickContext<'_>, products: &[ControlProduct]) -> OutputPlanKey {
        let patches = products.iter().fold(0u64, |sum, product| {
            sum.wrapping_add(ctx.control_patch_generation(*product))
        });
        (
            ctx.structure_epoch(),
            ctx.output_identity_revision(),
            patches,
        )
    }

    /// Plan `products` for this output. Asks each producer for its patch as
    /// this output sees it — the producers' produce, which resolved the
    /// patch, ran in this frame's input resolve.
    pub(crate) fn build(ctx: &TickContext<'_>, products: &[ControlProduct]) -> Self {
        let placements: Vec<FragmentPlacement> = products
            .iter()
            .map(|product| FragmentPlacement {
                patch: ctx.control_patch_placement(*product),
                product: *product,
            })
            .collect();
        let fragments = plan_fragments(&placements);
        let coverage = fragment_coverage(&fragments);
        let status = coverage_status(&coverage);
        Self {
            products: products.to_vec(),
            fragments,
            coverage,
            status,
        }
    }
}
