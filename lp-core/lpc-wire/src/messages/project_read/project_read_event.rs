//! Project-read event vocabulary.
//!
//! A single [`ProjectReadRequest`](super::ProjectReadRequest) can produce many
//! ordered [`ProjectReadEvent`] values. Transports batch those events into
//! [`ProjectReadFrame`](super::ProjectReadFrame) messages so project reads stay
//! bounded across serial, browser, and socket transports.
//!
//! Events are the semantic stream. Frame boundaries are deliberately invisible
//! at this layer: a resource payload, shape registry, or node tree may be split
//! across many frames, but the client observes one ordered event stream.

use alloc::string::String;
use alloc::vec::Vec;

use lpc_model::{ResourceRef, Revision, SlotShapeEntry, SlotShapeId};

use crate::project::{
    WireResourceSummary, WireRuntimeBufferMetadataPayload, WireRuntimeBufferPayload,
};
use crate::slot::WireSlotRootSnapshot;
use crate::tree::WireTreeDelta;

use super::{ProjectProbeResult, ReadLevel, RuntimeReadResult};

/// One ordered event in a project-read stream.
///
/// Every successful stream begins with [`ProjectReadEvent::Begin`] and ends with
/// [`ProjectReadEvent::End`]. A failed stream ends with
/// [`ProjectReadEvent::Error`]. Query and probe indexes are positions in the
/// original [`ProjectReadRequest`](super::ProjectReadRequest), which lets the
/// server stream results in whatever order is cheapest while preserving the
/// aggregate request/response contract for collectors.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectReadEvent {
    /// Starts one project-read event stream.
    Begin { revision: Revision },
    /// One event for the query at `ProjectReadRequest::queries[index]`.
    Query {
        index: u32,
        event: ProjectReadQueryEvent,
    },
    /// One event for the probe at `ProjectReadRequest::probes[index]`.
    Probe {
        index: u32,
        event: ProjectReadProbeEvent,
    },
    /// Ends one successful project-read event stream.
    End { revision: Revision },
    /// Ends one failed project-read event stream.
    Error { message: String },
}

/// Query-scoped project-read event.
///
/// Each variant corresponds to one [`ProjectReadQuery`](super::ProjectReadQuery)
/// family. Structured query families use nested begin/body/end events so a
/// server can emit them incrementally without allocating the whole result.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectReadQueryEvent {
    Shapes(ProjectReadShapeEvent),
    Nodes(ProjectReadNodeEvent),
    Resources(ProjectReadResourceEvent),
    Runtime(RuntimeReadResult),
}

/// Shape registry stream event.
///
/// Shape reads used to rely on public pagination to stay under transport
/// limits. They now stream entries directly, which keeps pagination out of the
/// semantic API while still bounding each frame.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectReadShapeEvent {
    Begin {
        level: ReadLevel,
        ids_revision: Revision,
    },
    Entry {
        id: SlotShapeId,
        entry: SlotShapeEntry,
    },
    End,
}

/// Node and node-slot stream event.
///
/// Tree deltas and slot-root snapshots can be emitted as they are produced by
/// the project mirror. Clients collect them into one node read result when they
/// need the compatibility aggregate.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectReadNodeEvent {
    Begin { level: ReadLevel },
    TreeDeltas { deltas: Vec<WireTreeDelta> },
    SlotRoot(WireSlotRootSnapshot),
    End,
}

/// Resource summary and payload stream event.
///
/// Small payloads may still use [`ProjectReadResourceEvent::RuntimeBufferPayload`].
/// Larger runtime buffers should use begin/bytes/end so firmware never needs to
/// build a single large JSON payload in memory.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectReadResourceEvent {
    Begin {
        level: ReadLevel,
    },
    Summary(WireResourceSummary),
    RuntimeBufferPayload(WireRuntimeBufferPayload),
    RuntimeBufferPayloadBegin {
        #[serde(rename = "ref")]
        resource_ref: ResourceRef,
        revision: Revision,
        metadata: WireRuntimeBufferMetadataPayload,
        byte_length: u32,
    },
    RuntimeBufferPayloadBytes {
        #[serde(rename = "ref")]
        resource_ref: ResourceRef,
        offset: u32,
        #[cfg_attr(feature = "schema-gen", schemars(with = "String"))]
        #[serde(with = "crate::serde_base64")]
        bytes: Vec<u8>,
    },
    RuntimeBufferPayloadEnd {
        #[serde(rename = "ref")]
        resource_ref: ResourceRef,
    },
    End,
}

/// Probe-scoped project-read event.
///
/// Probes are read-time diagnostics or render/sample requests that are not part
/// of the persistent project mirror. They are indexed separately from queries.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ProjectReadProbeEvent {
    Result(ProjectProbeResult),
}
