//! Composite gesture descriptors for config slot rows.
//!
//! Map and enum slots carry the structural facts their gesture affordances
//! need (M3 decision D1: gestures ARE the wire ops — the client dispatches
//! `EnsurePresent`/`RemoveValue` addresses and the server constructs all
//! defaults). Option gestures need no descriptor: `UiSlotOptionality` plus
//! the option row's address (and its `some` child) already suffice.

use lpc_model::SlotMapKey;

/// Structural gesture surface for a composite config slot row.
#[derive(Clone, Debug, PartialEq)]
pub enum UiSlotComposite {
    /// A map slot: entries can be added by key and removed per entry.
    Map(UiSlotMapComposite),
    /// An enum slot: the active variant can be switched to any declared one.
    Enum(UiSlotEnumComposite),
}

/// Gesture facts for a map slot row.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSlotMapComposite {
    /// Key domain of the map, typing the add-entry key input.
    pub key_kind: UiSlotMapKeyKind,
    /// Prefill for the add-entry key input: the next free index for numeric
    /// key maps, empty for string key maps.
    pub suggested_key: String,
}

/// Gesture facts for an enum slot row.
///
/// Variant names are the RAW declared idents (`PathPoints`, not
/// `path_points`) — slot paths address variants verbatim.
#[derive(Clone, Debug, PartialEq)]
pub struct UiSlotEnumComposite {
    /// Currently active variant ident.
    pub active: String,
    /// All declared variant idents, in declaration order.
    pub variants: Vec<String>,
}

/// Key domain for a map slot, mirroring `lpc_model::SlotMapKeyShape`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UiSlotMapKeyKind {
    String,
    I32,
    U32,
}

impl UiSlotMapKeyKind {
    /// True when the key input should be a numeric input.
    pub fn is_numeric(self) -> bool {
        matches!(self, Self::I32 | Self::U32)
    }

    /// Parse raw key input into a typed map key. `None` means "do not
    /// dispatch": empty strings and unparseable numbers never become keys.
    pub fn parse_key(self, raw: &str) -> Option<SlotMapKey> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        match self {
            Self::String => Some(SlotMapKey::String(raw.to_string())),
            Self::I32 => raw.parse().ok().map(SlotMapKey::I32),
            Self::U32 => raw.parse().ok().map(SlotMapKey::U32),
        }
    }
}

#[cfg(test)]
mod tests {
    use lpc_model::SlotMapKey;

    use super::UiSlotMapKeyKind;

    #[test]
    fn parses_typed_map_keys() {
        assert_eq!(
            UiSlotMapKeyKind::String.parse_key(" ring a "),
            Some(SlotMapKey::String("ring a".to_string()))
        );
        assert_eq!(
            UiSlotMapKeyKind::U32.parse_key("3"),
            Some(SlotMapKey::U32(3))
        );
        assert_eq!(
            UiSlotMapKeyKind::I32.parse_key("-2"),
            Some(SlotMapKey::I32(-2))
        );
    }

    #[test]
    fn rejects_empty_and_untyped_key_input() {
        assert_eq!(UiSlotMapKeyKind::String.parse_key("   "), None);
        assert_eq!(UiSlotMapKeyKind::U32.parse_key(""), None);
        assert_eq!(UiSlotMapKeyKind::U32.parse_key("-1"), None);
        assert_eq!(UiSlotMapKeyKind::U32.parse_key("1.5"), None);
        assert_eq!(UiSlotMapKeyKind::I32.parse_key("abc"), None);
    }

    #[test]
    fn numeric_kinds_are_numeric() {
        assert!(UiSlotMapKeyKind::U32.is_numeric());
        assert!(UiSlotMapKeyKind::I32.is_numeric());
        assert!(!UiSlotMapKeyKind::String.is_numeric());
    }
}
