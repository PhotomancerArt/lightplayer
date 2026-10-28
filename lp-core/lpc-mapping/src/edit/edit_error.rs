//! Why an edit was refused, in plain words. A refused edit changes nothing.

use alloc::string::String;
use alloc::vec::Vec;

use crate::part::{PartId, RenderError, SegmentKey, TreeId};

#[derive(Debug, Clone, PartialEq)]
pub enum EditError {
    /// No part has this id.
    NoSuchPart(PartId),
    /// Another part has this id.
    IdTaken(PartId),
    /// This id was minted before, or an override still names it: a part
    /// that took it would quietly pick up what pointed at the old one (D6).
    IdUsedBefore(PartId),
    /// The id an edit would give a part isn't one (too long, say).
    BadId {
        id: String,
        reason: String,
    },
    /// Parts can be put only at the top or into a group.
    NotAGroup(PartId),
    NotARepeat(PartId),
    NotACircle(PartId),
    NotAPath(PartId),
    /// A repeat holds exactly one part: that part can't be removed or moved
    /// out on its own (unrepeat the repeat, or remove it).
    HeldByRepeat(PartId),
    /// A part can't be moved into itself, or into a part under it.
    IntoItself(PartId),
    /// There is no place `index` in a list of `len` parts.
    IndexOutOfRange {
        index: usize,
        len: usize,
    },
    /// A repeat makes 1 to [`crate::MAX_REPEAT_COUNT`] copies.
    BadCount(u32),
    /// A ring that can't be drawn (why, in words).
    BadRing(String),
    /// A part holds at least one lamp.
    NoLamps(PartId),
    /// The part has no lamp count of its own (a group, a repeat, a grid…).
    NoLampCount(PartId),
    /// The circle has no ring with this key.
    NoSuchRing {
        part: PartId,
        key: SegmentKey,
    },
    /// The edit would make these overrides name other nodes than the ones
    /// they were set on (D12): clear them first.
    WouldRetarget {
        part: PartId,
        overrides: Vec<TreeId>,
    },
    /// A flow belongs to a part (or the fixture), never to one node.
    FlowOnNode(TreeId),
    /// The mapping must render for this edit, and doesn't.
    Render(RenderError),
}

impl core::fmt::Display for EditError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NoSuchPart(id) => write!(f, "there is no part {id}"),
            Self::IdTaken(id) => write!(f, "another part is already called {id}"),
            Self::IdUsedBefore(id) => write!(
                f,
                "{id} was used before; a part that took it would pick up what pointed at the old one"
            ),
            Self::BadId { id, reason } => write!(f, "{id:?} can't be an id: {reason}"),
            Self::NotAGroup(id) => write!(f, "{id} isn't a group, so it can't hold parts"),
            Self::NotARepeat(id) => write!(f, "{id} isn't a repeat"),
            Self::NotACircle(id) => write!(f, "{id} isn't a circle"),
            Self::NotAPath(id) => write!(f, "{id} isn't a path"),
            Self::HeldByRepeat(id) => write!(
                f,
                "{id} is what its repeat repeats: unrepeat or remove the repeat instead"
            ),
            Self::IntoItself(id) => write!(f, "{id} can't go inside itself"),
            Self::IndexOutOfRange { index, len } => {
                write!(f, "there is no place {index} among {len} parts")
            }
            Self::BadCount(count) => write!(
                f,
                "a repeat makes 1 to {} copies, not {count}",
                crate::MAX_REPEAT_COUNT
            ),
            Self::BadRing(reason) => write!(f, "{reason}"),
            Self::NoLamps(id) => write!(f, "{id} holds at least one lamp"),
            Self::NoLampCount(id) => write!(f, "{id} has no lamp count of its own"),
            Self::NoSuchRing { part, key } => write!(f, "{part} has no ring {key}"),
            Self::WouldRetarget { part, overrides } => {
                write!(f, "that would change which copies of {part} ")?;
                if let [one] = overrides.as_slice() {
                    write!(f, "the override on {one} names")?;
                } else {
                    write!(f, "{} overrides name", overrides.len())?;
                }
                write!(f, "; clear them first")
            }
            Self::FlowOnNode(id) => {
                write!(f, "a flow belongs to a part, and {id} is one node of it")
            }
            Self::Render(error) => write!(f, "{error}"),
        }
    }
}

impl core::error::Error for EditError {}
