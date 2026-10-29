//! A control product: what a fixture publishes on its control channel each
//! frame — one colour per lamp, in data order (its objects end to end, in
//! wiring order), after its overrides (the product's `ControlProduct`,
//! D55).
//!
//! It is the one place a lamp's colour comes from. The canvas draws each
//! lamp in its colour here; an output places the lamps on its pixels by the
//! patch and then paints its own overrides over its own buffer, which never
//! come back here.

use crate::domain::node::NodeId;
use crate::domain::rgb::Rgb;

/// One fixture's control product, as a frame holds it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlProduct<'a> {
    pub fixture: &'a NodeId,
    /// Per lamp, in data order. `None` for a lamp with no colour: outside
    /// its fixture's box (D4), or in a fixture nothing renders for.
    pub lamps: &'a [Option<Rgb>],
}
