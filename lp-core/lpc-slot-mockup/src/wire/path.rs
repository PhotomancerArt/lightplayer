use lpc_model::{SlotMapKey, SlotName};

pub(super) fn slot_name_for_key(key: &SlotMapKey) -> SlotName {
    SlotName::parse(&key_segment(key)).unwrap()
}

pub(super) fn key_segment(key: &SlotMapKey) -> String {
    match key {
        SlotMapKey::String(s) => s.clone(),
        SlotMapKey::I32(n) => n.to_string(),
        SlotMapKey::U32(n) => n.to_string(),
    }
}
