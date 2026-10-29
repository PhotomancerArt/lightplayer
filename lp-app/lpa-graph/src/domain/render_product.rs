//! A render product: a picture you can sample at a point and a time — what
//! a render source publishes on its visual channel, and what a fixture
//! samples at its lamps (the product's `VisualProduct`, D55).
//!
//! In the product it is a texture a shader renders; here it is the
//! source's stand-in [`Pattern`](crate::domain::pattern::Pattern). A fixture
//! samples it in its own box, as the unit square: `(0, 0)` its top-left
//! corner, `(1, 1)` its bottom-right (`Fixture::uv`).

use crate::domain::geom::Vec2;
use crate::domain::node::NodeId;
use crate::domain::pattern::Pattern;
use crate::domain::project::Project;
use crate::domain::rgb::Rgb;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderProduct {
    /// The render source that publishes it.
    pub source: NodeId,
    pub pattern: Pattern,
}

impl RenderProduct {
    /// Its colour at `uv` (the unit square) at `time` seconds.
    pub fn sample(&self, uv: Vec2, time: f64) -> Rgb {
        self.pattern.sample(uv, time)
    }
}

impl Project {
    /// The render product a fixture samples: its visual channel's, from
    /// the nearest writer (`bindings.rs`). `None` when nothing writes it.
    pub fn render_product(&self, fixture: &NodeId) -> Option<RenderProduct> {
        let source = self.visual_source(fixture)?;
        let pattern = self.node(&source)?.source()?.pattern;
        Some(RenderProduct { source, pattern })
    }
}
