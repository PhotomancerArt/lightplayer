use alloc::string::{String, ToString};

use lpc_model::{SlotPath, SlotPathError};

use crate::edit::SlotEdit;

/// Stable wire/display key for a slot path in pending maps.
pub fn slot_path_key(path: &SlotPath) -> String {
    path.to_string()
}

/// Canonical overlay key for one pending [`SlotEdit`].
pub fn slot_edit_key(edit: &SlotEdit) -> String {
    match edit {
        SlotEdit::MapInsert { path, key, .. } => {
            alloc::format!("{}#map_insert:{key}", slot_path_key(path))
        }
        SlotEdit::MapRemove { path, key, .. } => {
            alloc::format!("{}#map_remove:{key}", slot_path_key(path))
        }
        SlotEdit::UseEnumVariant { path, .. }
        | SlotEdit::AssignValue { path, .. }
        | SlotEdit::UseOption { path, .. } => slot_path_key(path),
    }
}

/// Parse a pending map key back to a [`SlotPath`].
pub fn parse_slot_path_key(key: &str) -> Result<SlotPath, SlotPathError> {
    if key.is_empty() {
        return Ok(SlotPath::root());
    }
    SlotPath::parse(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::SlotPath;

    #[test]
    fn round_trip_root_and_nested_paths() {
        let root = SlotPath::root();
        assert_eq!(parse_slot_path_key(&slot_path_key(&root)).unwrap(), root);

        for raw in [
            "controls.rate",
            "entries[2].node",
            r#"params["phase.offset"].label"#,
        ] {
            let path = SlotPath::parse(raw).unwrap();
            let key = slot_path_key(&path);
            assert_eq!(parse_slot_path_key(&key).unwrap(), path);
        }
    }
}
