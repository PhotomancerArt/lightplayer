//! Request-scoped diagnostic probes.

mod binding_graph_probe;
mod control_product_probe;
mod geometry_gate;
mod output_frame_probe;
mod project_probe;
mod render_product_probe;
mod revision_gate;
mod timebase_probe;

pub use binding_graph_probe::{
    BindingGraphProbeRequest, BindingGraphProbeResult, WireBindingDirection, WireBindingEndpoint,
    WireBindingGraph, WireBindingGraphRead, WireBindingOrigin, WireBusChannel, WireBusChannelValue,
    WireBusChannelValues, WireEffectiveBinding, WireScopeRef,
};
pub use control_product_probe::{
    ControlProductGeometry, ControlProductProbeRequest, ControlProductProbeResult,
    ControlProductProbeResultHeader,
};
pub use geometry_gate::GeometryDisplayLayout;
pub use output_frame_probe::{
    OutputFrameEntry, OutputFrameEntryHeader, OutputFrameGeometry, OutputFrameProbeRequest,
    OutputFrameProbeResult, OutputFrameProbeResultHeader, WireOutputPlacement,
};
pub use project_probe::{ProjectProbeRequest, ProjectProbeResult, ProjectProbeResultHeader};
pub use render_product_probe::{
    RenderProductProbeRequest, RenderProductProbeResult, RenderProductProbeResultHeader,
    WireCellProjection, WireConsumerPolicy, WireProjectionOrigin, WireProjectionShape,
    WireVisualSpace,
};
pub use revision_gate::{KnownRevision, RevisionGateRead, RevisionGateResult};
pub use timebase_probe::{
    TimebaseProbeRequest, TimebaseProbeResult, WirePhasorOrigin, WirePhasorReading, WirePhasorRow,
};
