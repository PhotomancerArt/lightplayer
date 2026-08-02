//! Typed sections inside a node tab.

use crate::{UiConfigSlot, UiNodeChild, UiProducedProduct, UiProducedValue};

/// A semantic section in a node tab body.
#[derive(Clone, Debug, PartialEq)]
pub enum UiNodeSection {
    /// Product outputs that power the main visual section.
    ProducedProducts(Vec<UiProducedProduct>),
    /// Non-product outputs, such as time or progress values.
    ProducedValues(Vec<UiProducedValue>),
    /// Normal configurable input slots — the **Settings** section (D6): the
    /// authored config that Save writes back.
    ConfigSlots(Vec<UiConfigSlot>),
    /// **Debug** slots (D3/D4): every `SlotRole::Debug` field of the node,
    /// **flattened** — the section comes from the ROLE, never from a record
    /// being named `controls`, so a clock's three `controls.*` fields render
    /// directly here with no nesting. Transient by nature: never dirty, never
    /// saved, cleared rather than reverted (D7).
    ///
    /// The section is rendered as debug territory *even when empty of
    /// overrides* — knowing a control is transient BEFORE touching it is the
    /// whole point (D8 tier c).
    DebugSlots(Vec<UiConfigSlot>),
    /// Asset slots promoted to editor-level treatment.
    AssetSlots(Vec<UiConfigSlot>),
    /// Children shown inline for small compositions or story isolation.
    Children(Vec<UiNodeChild>),
}

impl UiNodeSection {
    /// Returns true when the section has no items.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::ProducedProducts(items) => items.is_empty(),
            Self::ProducedValues(items) => items.is_empty(),
            Self::ConfigSlots(items) => items.is_empty(),
            Self::DebugSlots(items) => items.is_empty(),
            Self::AssetSlots(items) => items.is_empty(),
            Self::Children(items) => items.is_empty(),
        }
    }
}
