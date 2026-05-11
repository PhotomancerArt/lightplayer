//! Engine↔client message envelope and payloads.

mod client;
mod envelope;
pub mod project_read;

pub use client::{ClientMessage, ClientRequest};
pub use envelope::{Message, NoDomain, ServerMessage};
pub use project_read::{
    ExplainSlotProbeRequest, ExplainSlotProbeResult, NodeReadQuery, NodeReadResult,
    NodeReadSelection, ProjectProbeRequest, ProjectProbeResult, ProjectReadQuery,
    ProjectReadRequest, ProjectReadResponse, ProjectReadResult, ReadLevel,
    RenderProductProbeRequest, RenderProductProbeResult, ResourcePayloadRead, ResourceReadQuery,
    ResourceReadResult, ShapeReadQuery, ShapeReadResult, SlotExplanation,
};
