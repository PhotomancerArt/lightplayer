//! What the mapping says about its lamps, minimally (D10): their colors,
//! their order on the line, and the name of a lamp spec.
//!
//! On the mapping (the fixture's default) and on any part, inherited down
//! the tree field by field: a segment's colors come from the nearest part
//! that says, else the mapping's, else RGB. Presets, tuning and power belong
//! to the lamp-model plan; `spec` is only a name kept for it.

use alloc::string::String;

use crate::part::colors::Colors;
use crate::part::order::Order;

/// Lamp facts, each optional: `None` takes what the part above says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LampRef {
    pub colors: Option<Colors>,
    pub order: Option<Order>,
    /// A lamp spec's name, kept for the lamp-model plan.
    pub spec: Option<String>,
}

impl LampRef {
    /// This one over `outer`: each field this one sets wins, the rest come
    /// from `outer`.
    #[must_use]
    pub fn over(&self, outer: &LampRef) -> LampRef {
        LampRef {
            colors: self.colors.or(outer.colors),
            order: self.order.or(outer.order),
            spec: self.spec.clone().or_else(|| outer.spec.clone()),
        }
    }

    /// The colors, RGB when nothing says.
    #[must_use]
    pub fn colors_or_rgb(&self) -> Colors {
        self.colors.unwrap_or_default()
    }

    /// Channels per lamp: its colors' channel count.
    #[must_use]
    pub fn channels_per_lamp(&self) -> u32 {
        self.colors_or_rgb().channels()
    }

    /// The order, checked against the colors: `Err` when it is set and its
    /// letters are not exactly the colors' channels. An unset order is fine
    /// (the engine supplies the fixture's own).
    pub fn checked_order(&self) -> Result<Option<Order>, Order> {
        match self.order {
            Some(order) if !order.fits(self.colors_or_rgb()) => Err(order),
            order => Ok(order),
        }
    }
}
