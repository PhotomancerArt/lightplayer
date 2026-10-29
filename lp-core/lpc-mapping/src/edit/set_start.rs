//! Set where the fixture, a part or one node starts.

use crate::edit::edit_error::EditError;
use crate::edit::part_place::part_mut;
use crate::part::Mapping;
use crate::patching::{ChannelOp, OutputOp, PatchTarget, PortOp, Start};

/// Set a start (one op per field; Keep takes what it would get).
///
/// - [`PatchTarget::Fixture`]: where the fixture starts.
/// - [`PatchTarget::Part`]: the part's own start, which **applies to its
///   node in every copy** of any repeat round it (F15).
/// - [`PatchTarget::Node`]: one node's start, in its override's output,
///   port and channel: **only that node** (a copy's node is that copy). A
///   Keep field clears that field of the override ("Back to the flow"), as
///   Keep in a part's start falls through to the flow; its other fields
///   (off, direction, offset) stay, and an override left empty is removed.
///   Refuses a node whose part isn't in the mapping.
pub fn set_start(m: &mut Mapping, target: PatchTarget, start: Start) -> Result<(), EditError> {
    match target {
        PatchTarget::Fixture => m.start = start,
        PatchTarget::Part(id) => {
            part_mut(m, &id).ok_or(EditError::NoSuchPart(id))?.start = start;
        }
        PatchTarget::Node(id) => {
            if m.part(&id.part).is_none() {
                return Err(EditError::NoSuchPart(id.part));
            }
            let entry = m.overrides.entry(id.clone()).or_default();
            entry.output = (start.output != OutputOp::Keep).then_some(start.output);
            entry.port = (start.port != PortOp::Keep).then_some(start.port);
            entry.channel = (start.channel != ChannelOp::Keep).then_some(start.channel);
            if entry.is_empty() {
                m.overrides.remove(&id);
            }
        }
    }
    Ok(())
}
