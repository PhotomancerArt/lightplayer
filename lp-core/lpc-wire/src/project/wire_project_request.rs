//! Wire-visible project request / node status types.

use alloc::string::String;
use lpc_model::project::FrameId;
use serde::{Deserialize, Serialize};

use super::{
    LegacyWireNodeSpecifier, RenderProductPayloadRequest, ResourceSummarySpecifier,
    RuntimeBufferPayloadSpecifier, WireSlotWatchSpecifier,
};

/// Project-scoped request from client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireProjectRequest {
    /// Incremental sync since a frame.
    GetChanges {
        /// Last frame the client synced.
        since_frame: FrameId,
        /// Which nodes need full detail.
        legacy_detail_specifier: LegacyWireNodeSpecifier,
        /// Which generic slot roots the client wants to watch.
        #[serde(default)]
        slot_watch_specifier: WireSlotWatchSpecifier,
        /// Which resource summary domains to include (per-request; no server-side subscription state).
        #[serde(default)]
        resource_summary_specifier: ResourceSummarySpecifier,
        /// Which runtime-buffer payloads to include.
        #[serde(default)]
        runtime_buffer_payload_specifier: RuntimeBufferPayloadSpecifier,
        /// Which render-product payloads to materialize (plus reserved future options).
        #[serde(default)]
        render_product_payload_request: RenderProductPayloadRequest,
    },
}

/// Node lifecycle / health status on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub enum WireNodeStatus {
    /// Created but not yet initialized.
    Created,
    /// Error initializing the node.
    InitError(String),
    /// Running normally.
    Ok,
    /// Running with a warning.
    Warn(String),
    /// Cannot run.
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LegacyWireNodeSpecifier;
    use alloc::vec;
    use lpc_model::node::NodeId;

    #[test]
    fn wire_node_specifier_round_trips() {
        let spec = LegacyWireNodeSpecifier::None;
        assert_eq!(spec, LegacyWireNodeSpecifier::None);

        let spec = LegacyWireNodeSpecifier::All;
        assert_eq!(spec, LegacyWireNodeSpecifier::All);

        let spec = LegacyWireNodeSpecifier::ByHandles(vec![NodeId::new(1), NodeId::new(2)]);
        match spec {
            LegacyWireNodeSpecifier::ByHandles(handles) => {
                assert_eq!(handles.len(), 2);
            }
            _ => panic!("Expected ByHandles"),
        }
    }

    #[test]
    fn wire_project_request_shape() {
        let request = WireProjectRequest::GetChanges {
            since_frame: FrameId::default(),
            legacy_detail_specifier: LegacyWireNodeSpecifier::All,
            slot_watch_specifier: WireSlotWatchSpecifier::None,
            resource_summary_specifier: ResourceSummarySpecifier::default(),
            runtime_buffer_payload_specifier: RuntimeBufferPayloadSpecifier::default(),
            render_product_payload_request: RenderProductPayloadRequest::default(),
        };
        match request {
            WireProjectRequest::GetChanges {
                since_frame,
                legacy_detail_specifier: detail_specifier,
                slot_watch_specifier,
                resource_summary_specifier,
                runtime_buffer_payload_specifier,
                render_product_payload_request,
            } => {
                assert_eq!(since_frame, FrameId::default());
                assert_eq!(detail_specifier, LegacyWireNodeSpecifier::All);
                assert_eq!(slot_watch_specifier, WireSlotWatchSpecifier::None);
                assert_eq!(resource_summary_specifier, ResourceSummarySpecifier::None);
                assert_eq!(
                    runtime_buffer_payload_specifier,
                    RuntimeBufferPayloadSpecifier::None
                );
                assert_eq!(
                    render_product_payload_request,
                    RenderProductPayloadRequest::default()
                );
            }
        }
    }

    #[test]
    fn wire_node_status_variants() {
        let status = WireNodeStatus::Created;
        assert_eq!(status, WireNodeStatus::Created);

        let status = WireNodeStatus::InitError("test error".into());
        match status {
            WireNodeStatus::InitError(msg) => assert_eq!(msg, "test error"),
            _ => panic!("Expected InitError"),
        }
    }
}
