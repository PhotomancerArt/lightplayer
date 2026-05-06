//! Integration surface for demo-style resource watches on `WireProjectRequest::GetChanges`.

use lpc_model::project::FrameId;
use lpc_wire::{
    LegacyWireNodeSpecifier, RenderProductPayloadRequest, RenderProductPayloadSpecifier,
    ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier, WireProjectRequest,
};

#[test]
fn get_changes_all_summaries_and_payload_specifiers_round_trip_json() {
    let req = WireProjectRequest::GetChanges {
        since_frame: FrameId::default(),
        legacy_detail_specifier: LegacyWireNodeSpecifier::All,
        slot_watch_specifier: Default::default(),
        resource_summary_specifier: ResourceSummarySpecifier::All,
        runtime_buffer_payload_specifier: RuntimeBufferPayloadSpecifier::All,
        render_product_payload_request: RenderProductPayloadRequest {
            specifier: RenderProductPayloadSpecifier::All,
            options: Default::default(),
        },
    };

    let j = serde_json::to_string(&req).expect("serialize project request");
    let back: WireProjectRequest = serde_json::from_str(&j).expect("deserialize project request");
    assert_eq!(req, back);
}
