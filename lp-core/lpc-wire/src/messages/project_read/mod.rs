//! Project-read request, event-frame, and compatibility response vocabulary.
//!
//! A project read is intentionally split into three layers:
//!
//! - [`ProjectReadRequest`] describes the semantic data the client wants.
//! - [`ProjectReadEvent`] describes ordered pieces of the answer.
//! - [`ProjectReadFrame`] batches those events into bounded transport messages.
//!
//! This exists because firmware transports cannot safely hold one large JSON
//! response for a full project mirror. Servers send multiple same-request-id
//! `ProjectReadFrame` messages instead. Clients that still want the older
//! aggregate shape can rebuild it with [`ProjectReadCollector`] and receive a
//! [`ProjectReadResponse`] once the stream ends.

mod node_read;
mod probe;
mod project_read_collector;
mod project_read_event;
mod project_read_frame;
mod project_read_request;
mod project_read_response;
mod read_level;
mod resource_read;
mod runtime_read;
mod shape_read;

pub use node_read::{NodeReadQuery, NodeReadResult, NodeReadSelection};
pub use probe::{
    ControlDisplayLayoutProbeResult, ControlDisplayLayoutRead, ControlProductProbeRequest,
    ControlProductProbeResult, ExplainSlotProbeRequest, ExplainSlotProbeResult,
    ProjectProbeRequest, ProjectProbeResult, RenderProductProbeRequest, RenderProductProbeResult,
    SlotExplanation,
};
pub use project_read_collector::{
    ProjectReadCollectError, ProjectReadCollectStatus, ProjectReadCollector,
};
pub use project_read_event::{
    ProjectReadEvent, ProjectReadNodeEvent, ProjectReadProbeEvent, ProjectReadQueryEvent,
    ProjectReadResourceEvent, ProjectReadShapeEvent,
};
pub use project_read_frame::{
    PROJECT_READ_FRAME_MAX_BYTES, PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES, ProjectReadFrame,
};
pub use project_read_request::{ProjectReadQuery, ProjectReadRequest};
pub use project_read_response::{ProjectReadResponse, ProjectReadResult};
pub use read_level::ReadLevel;
pub use resource_read::{ResourcePayloadRead, ResourceReadQuery, ResourceReadResult};
pub use runtime_read::{
    ProjectRuntimeStatus, RuntimeReadQuery, RuntimeReadResult, ServerRuntimeStatus,
};
pub use shape_read::{ShapeReadQuery, ShapeReadResult};
