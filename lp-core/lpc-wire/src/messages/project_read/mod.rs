//! Project-read request, event-frame, and compatibility response vocabulary.
//!
//! A project read is intentionally split into three layers:
//!
//! - [`ProjectReadRequest`] describes the semantic data the client wants.
//! - [`ProjectReadEvent`] describes ordered pieces of the answer.
//!
//! This exists because firmware transports cannot safely hold one large JSON
//! response for a full project mirror. Servers stream those events directly in
//! `ServerMsgBody::ProjectRead` messages, batched to a transport budget and
//! sequenced by the envelope (`seq`/`fin`). Clients that still want the older
//! aggregate shape can rebuild it with [`ProjectReadCollector`] and receive a
//! [`ProjectReadResponse`] once the stream ends.

mod node_read;
mod probe;
mod project_read_collector;
mod project_read_event;
mod project_read_request;
mod project_read_response;
mod read_level;
mod resource_read;
mod runtime_read;
mod shape_read;

pub use crate::budget::{
    PROJECT_READ_FRAME_MAX_BYTES, PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES,
    PROJECT_READ_FRAME_SERIAL_MARGIN_BYTES, PROJECT_READ_RUNTIME_CHUNK_BYTES,
};
pub use node_read::{NodeReadQuery, NodeReadResult, NodeReadSelection};
pub use probe::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, ControlProductProbeRequest,
    ControlProductProbeResult, ControlProductProbeResultHeader, ExplainSlotProbeRequest,
    ExplainSlotProbeResult, ProjectProbeRequest, ProjectProbeResult, ProjectProbeResultHeader,
    RenderProductProbeRequest, RenderProductProbeResult, RenderProductProbeResultHeader,
    SlotExplanation,
};
pub use project_read_collector::{
    ProjectReadCollectError, ProjectReadCollectStatus, ProjectReadCollector,
};
pub use project_read_event::{
    ProjectReadEvent, ProjectReadNodeEvent, ProjectReadProbeEvent, ProjectReadQueryEvent,
    ProjectReadResourceEvent, ProjectReadShapeEvent,
};
pub use project_read_request::{ProjectReadQuery, ProjectReadRequest};
pub use project_read_response::{ProjectReadResponse, ProjectReadResult};
pub use read_level::ReadLevel;
pub use resource_read::{ResourcePayloadRead, ResourceReadQuery, ResourceReadResult};
pub use runtime_read::{
    ProjectRuntimeStatus, RuntimeReadQuery, RuntimeReadResult, ServerRuntimeStatus,
};
pub use shape_read::{ShapeReadQuery, ShapeReadResult};
