//! The canvas's render layers, bottom → top: fixture sprites (project
//! space), doc content (reference, authored rect, arrows, lamps, numbers),
//! selection furniture, the path draft, and the marquee. Each layer is a
//! plain function returning the
//! layer's SVG children — the canvas composes them inside its camera ∘
//! placement groups, so splitting changes nothing about the emitted DOM.

pub(crate) mod cells;
pub(crate) mod doc;
pub(crate) mod draft;
pub(crate) mod fixtures;
pub(crate) mod hull;
pub(crate) mod marquee;
pub(crate) mod outline;
pub(crate) mod selection;
