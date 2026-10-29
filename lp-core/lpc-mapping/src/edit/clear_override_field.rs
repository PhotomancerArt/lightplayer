//! Clear one field of a node's override ("Back to the flow").

use crate::edit::edit_error::EditError;
use crate::edit::override_field::OverrideFieldKind;
use crate::part::{Mapping, TreeId};

/// Clear one field of the override on the node `id`: the node takes what
/// its part's start and its parent's flow give it for that field. An
/// override left with nothing set is removed. Clearing what isn't set does
/// nothing, and an override whose node is gone is cleared like any other,
/// so it never refuses (it returns a `Result` like every edit).
pub fn clear_override_field(
    m: &mut Mapping,
    id: &TreeId,
    field: OverrideFieldKind,
) -> Result<(), EditError> {
    if let Some(entry) = m.overrides.get_mut(id) {
        field.clear(entry);
        if entry.is_empty() {
            m.overrides.remove(id);
        }
    }
    Ok(())
}
