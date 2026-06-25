//! Typed sections inside a node tab.

use crate::{UiConfigAsset, UiConfigSlot, UiNodeChild, UiProducedProduct, UiProducedValue};

/// A semantic section in a node tab body.
#[derive(Clone, Debug, PartialEq)]
pub enum UiNodeSection {
    /// Product outputs that power the main visual section.
    ProducedProducts(Vec<UiProducedProduct>),
    /// Non-product outputs, such as time or progress values.
    ProducedValues(Vec<UiProducedValue>),
    /// Normal configurable input slots.
    ConfigSlots(Vec<UiConfigSlot>),
    /// Asset slots promoted to editor-level treatment.
    ConfigAssets(Vec<UiConfigAsset>),
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
            Self::ConfigAssets(items) => items.is_empty(),
            Self::Children(items) => items.is_empty(),
        }
    }
}
