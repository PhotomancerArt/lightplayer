//! Bounded transport frame for project-read events.
//!
//! A [`ProjectReadFrame`] is the transport batching layer for a project read.
//! It has no domain meaning beyond "these ordered events fit in one server
//! message." Servers are free to choose frame boundaries based on transport
//! budget, so clients must concatenate frames by `sequence` and interpret only
//! the contained [`ProjectReadEvent`] values.

use alloc::vec::Vec;

use super::ProjectReadEvent;

/// Target maximum encoded JSON size for one project-read server message.
///
/// The frame batcher measures the encoded `WireServerMessage` body against
/// this budget. Tiny transport delimiters such as `M!` and the trailing newline
/// are intentionally excluded.
pub const PROJECT_READ_FRAME_MAX_BYTES: usize = 16 * 1024;

/// Minimum server-side scratch buffer for serializing one project-read frame.
///
/// Firmware transports can use this for their stack serialization buffer while
/// still asking the shared batcher to keep each JSON message under
/// [`PROJECT_READ_FRAME_MAX_BYTES`]. The small margin covers framing delimiters
/// and serializer bookkeeping; it is not intended to raise the transport frame
/// budget.
pub const PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES: usize = PROJECT_READ_FRAME_MAX_BYTES + 256;

/// One transport-level batch of project-read events.
///
/// Frames are correlated to the original request by the outer server message id,
/// not by data inside this struct. `sequence` starts at zero for each project
/// read and must increase by one. The final frame is whichever frame carries an
/// [`ProjectReadEvent::End`] or [`ProjectReadEvent::Error`] event.
///
/// Keep this type small and transport-focused. Add new project-read meaning to
/// [`ProjectReadEvent`] or one of its scoped event enums instead.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct ProjectReadFrame {
    /// Monotonic frame number within one request stream.
    pub sequence: u32,
    /// Ordered events carried by this bounded transport batch.
    pub events: Vec<ProjectReadEvent>,
}

impl ProjectReadFrame {
    #[must_use]
    pub fn new(sequence: u32, events: Vec<ProjectReadEvent>) -> Self {
        Self { sequence, events }
    }
}
