//! Typed sections inside a node tab.

use crate::{UiConsumedAsset, UiConsumedSlot, UiNodeChild, UiProducedProduct, UiProducedValue};

/// A semantic section in a node tab body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UiNodeSection {
    /// Product outputs that power the main visual section.
    ProducedProducts(Vec<UiProducedProduct>),
    /// Non-product outputs, such as time or progress values.
    ProducedValues(Vec<UiProducedValue>),
    /// Normal consumed configuration and runtime inputs.
    ConsumedValues(Vec<UiConsumedSlot>),
    /// Asset slots promoted to editor-level treatment.
    ConsumedAssets(Vec<UiConsumedAsset>),
    /// Children shown inline for small compositions or story isolation.
    Children(Vec<UiNodeChild>),
}

impl UiNodeSection {
    /// Returns true when the section has no items.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::ProducedProducts(items) => items.is_empty(),
            Self::ProducedValues(items) => items.is_empty(),
            Self::ConsumedValues(items) => items.is_empty(),
            Self::ConsumedAssets(items) => items.is_empty(),
            Self::Children(items) => items.is_empty(),
        }
    }
}
