//! The top layer: what you change by turning a control.
//!
//! Temporary by design. The first change gives the layer an end — the
//! schedule's next change — and the whole layer drops then.

use std::collections::BTreeMap;

use crate::control::{ControlId, Value};
use crate::time::Minute;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overrides {
    pub pins: BTreeMap<ControlId, Value>,
    /// `None` while empty, or while the board does not know the time yet
    /// (the end is decided once it does).
    pub ends: Option<Minute>,
}

impl Overrides {
    pub fn is_empty(&self) -> bool {
        self.pins.is_empty()
    }
}
