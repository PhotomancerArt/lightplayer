//! Set one field of a node's override.

use crate::edit::edit_error::EditError;
use crate::edit::override_field::OverrideField;
use crate::part::{Mapping, TreeId};

/// Set one field of the override on the node `id`, making the override if
/// there is none; the other fields stay. An override left with nothing set
/// (`Off(false)` on an otherwise empty one) is removed.
///
/// Refuses a node whose part isn't in the mapping. The rest of the id (a
/// ring's key, a copy) isn't checked here, which would need a render: an
/// override on a node that isn't there is reported by the patcher as gone.
pub fn set_override_field(
    m: &mut Mapping,
    id: &TreeId,
    field: OverrideField,
) -> Result<(), EditError> {
    if m.part(&id.part).is_none() {
        return Err(EditError::NoSuchPart(id.part.clone()));
    }
    let entry = m.overrides.entry(id.clone()).or_default();
    field.apply(entry);
    if entry.is_empty() {
        m.overrides.remove(id);
    }
    Ok(())
}
