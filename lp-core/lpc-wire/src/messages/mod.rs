//! Engine↔client message envelope and payloads.

pub mod project_read;

pub use crate::message::client::{ClientMessage, ClientRequest};
pub use crate::message::envelope::{Message, ServerMessage};
pub use project_read::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, ControlProductProbeRequest,
    ControlProductProbeResult, ExplainSlotProbeRequest, ExplainSlotProbeResult, NodeReadQuery,
    NodeReadResult, NodeReadSelection, PROJECT_READ_FRAME_MAX_BYTES,
    PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES, PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES,
    PROJECT_READ_RUNTIME_CHUNK_BYTES, ProjectProbeRequest, ProjectProbeResult,
    ProjectReadCollectError, ProjectReadCollectStatus, ProjectReadCollector, ProjectReadEvent,
    ProjectReadFrame, ProjectReadNodeEvent, ProjectReadProbeEvent, ProjectReadQuery,
    ProjectReadQueryEvent, ProjectReadRequest, ProjectReadResourceEvent, ProjectReadResponse,
    ProjectReadResult, ProjectReadShapeEvent, ProjectRuntimeStatus, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, ResourcePayloadRead, ResourceReadQuery,
    ResourceReadResult, RuntimeReadQuery, RuntimeReadResult, ServerRuntimeStatus, ShapeReadQuery,
    ShapeReadResult, SlotExplanation,
};
