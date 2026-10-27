//! Why a mapping didn't render: a part that can't be drawn, two parts with
//! one id, or more lamps than memory holds.

use alloc::string::{String, ToString};

use crate::map2d_error::Map2dError;
use crate::part::part_id::PartId;

#[derive(Debug, Clone, PartialEq)]
pub enum RenderError {
    /// One part's numbers can't be drawn (a path of no length, a grid of no
    /// columns, a repeat of no copies).
    InvalidPart { part: PartId, reason: String },
    /// Two parts claim one id: anything pointing at it would be ambiguous.
    DuplicateId(PartId),
    /// The lamps' positions are one buffer, and the heap could not hand
    /// over one that size: refused instead of aborting (as
    /// [`crate::resolve_into`] does).
    TooBigForMemory { lamps: u32, bytes: u32 },
}

impl RenderError {
    /// Today's shape error, said of a part.
    pub(crate) fn from_shape(part: &PartId, error: Map2dError) -> Self {
        let reason = match error {
            Map2dError::InvalidObject { reason, .. } => reason,
            other => other.to_string(),
        };
        RenderError::InvalidPart {
            part: part.clone(),
            reason,
        }
    }
}

impl core::fmt::Display for RenderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidPart { part, reason } => write!(f, "part {part}: {reason}"),
            Self::DuplicateId(id) => write!(f, "two parts are called {id}"),
            Self::TooBigForMemory { lamps, bytes } => write!(
                f,
                "{lamps} lamps need {bytes} B in one piece, more than this board's memory has free"
            ),
        }
    }
}

impl core::error::Error for RenderError {}
