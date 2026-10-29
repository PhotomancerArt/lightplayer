//! Set how the fixture's or a part's children follow each other.

use crate::edit::edit_error::EditError;
use crate::edit::part_place::part_mut;
use crate::part::Mapping;
use crate::patching::{Flow, PatchTarget};

/// Set a flow: the fixture's (how its top-level parts follow each other) or
/// a part's (a group's parts, a repeat's copies, a circle's rings, a split
/// path's subpaths), the same in every copy of the part. A flow renders a
/// start for each child and never moves the cursor itself (D4), so the
/// children's overrides still win over it.
///
/// Refuses a node: a flow belongs to a part, never to one node of it.
pub fn set_flow(m: &mut Mapping, target: PatchTarget, flow: Flow) -> Result<(), EditError> {
    match target {
        PatchTarget::Fixture => m.flow = flow,
        PatchTarget::Part(id) => {
            part_mut(m, &id).ok_or(EditError::NoSuchPart(id))?.flow = flow;
        }
        PatchTarget::Node(id) => return Err(EditError::FlowOnNode(id)),
    }
    Ok(())
}
