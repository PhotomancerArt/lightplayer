//! Which level a click selects: the Figma-style drill-down (D17).
//!
//! The cursor always hits the deepest thing (`hit_test.rs`). Of the chain
//! from the top level down to it, a click selects:
//!
//! - **nothing selected, or a top-level thing selected:** the top-level
//!   thing under the cursor;
//! - **something deeper selected:** the thing under the cursor at the same
//!   level, inside the deepest group/circle/ring you are already in. Click a
//!   lamp of ring B while a lamp of ring A is selected and you get ring B;
//!   click another lamp of ring A and you get that lamp;
//! - **the thing you click is already the (only) selection:** one level
//!   deeper — "click again to go in";
//! - **⌘-click:** the deepest thing, directly.
//!
//! Hover shows exactly what a click would select. That is the whole
//! teaching device, so it is ONE function used by both.

use crate::fixture::Fixture;
use crate::target::Target;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickMode {
    /// What a plain click selects, drilling in when you click the selection.
    Click,
    /// What a press selects before it knows whether it is a drag: never
    /// drills (pressing on the selection keeps it, so you can drag it).
    Press,
    /// ⌘-click: the deepest thing under the cursor.
    Deep,
}

pub fn pick(fixture: &Fixture, selection: &[Target], hit: &Target, mode: PickMode) -> Target {
    let chain = fixture.chain(hit);
    if mode == PickMode::Deep {
        return hit.clone();
    }

    // The context: where the current selection lives (`None` = top level).
    let context = selection.first().and_then(|s| fixture.parent(s));
    let anchors = context.map(|c| fixture.chain(&c)).unwrap_or_default();

    // The deepest thing in the hit's chain that is the context or above it;
    // the candidate is the next one down.
    let k = chain.iter().rposition(|t| anchors.contains(t));
    let mut idx = k.map_or(0, |k| (k + 1).min(chain.len() - 1));

    if mode == PickMode::Click
        && selection.len() == 1
        && selection[0] == chain[idx]
        && idx + 1 < chain.len()
    {
        idx += 1;
    }
    chain[idx].clone()
}
