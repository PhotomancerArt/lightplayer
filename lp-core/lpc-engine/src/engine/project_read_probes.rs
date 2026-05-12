//! Project probe helpers.

use alloc::format;

use lpc_wire::{
    ExplainSlotProbeRequest, ExplainSlotProbeResult, RenderProductProbeRequest,
    RenderProductProbeResult, SlotExplanation,
};

use super::Engine;

impl Engine {
    pub(super) fn read_project_render_product_probe(
        &self,
        request: RenderProductProbeRequest,
    ) -> RenderProductProbeResult {
        let _ = request;
        RenderProductProbeResult::Unsupported {
            reason: format!("render product probe execution is not implemented yet"),
        }
    }

    pub(super) fn read_project_explain_slot_probe(
        &self,
        request: ExplainSlotProbeRequest,
    ) -> ExplainSlotProbeResult {
        let _ = SlotExplanation {
            value: None,
            trace: alloc::vec::Vec::new(),
        };
        ExplainSlotProbeResult::Unsupported {
            reason: format!(
                "explain slot probe execution is not implemented yet for node {:?} slot {:?}",
                request.node, request.slot
            ),
        }
    }
}
