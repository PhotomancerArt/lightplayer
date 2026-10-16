//! Arrows for the mapping lab: a semantic arrow in, renderer-agnostic
//! geometry out.
//!
//! The lab's canvas draws wiring as curved arrows with words on them, and
//! the words were the messy part: a label floating at the midpoint, level,
//! wherever the curve happened to be, and another one coming for every
//! channel's name. This crate is the arrow piece of the lab's view model
//! (the model says *what* to draw, with roles; the renderer only draws):
//!
//! - an [`Arrow`] says what the arrow *is* — from, to, how it leaves and
//!   arrives ([`EndDirection`]), its [`Route`], its [`Head`]s, and its
//!   [`Label`]s with a [`LabelRole`] and a preferred [`Placement`];
//! - [`Arrow::layout`] or [`layout_arrows`] turn arrows into
//!   [`ArrowGeometry`]: the line as [`Path`] segments (and an SVG `d`),
//!   each head as a polygon with the line cut back so it doesn't poke
//!   through, and each label as a [`PlacedLabel`] — a little box lying
//!   along the line, upright, clear of the heads and of the other labels —
//!   or a [`LabelFit::Dropped`] with its [`DropReason`].
//!
//! It measures text through a [`TextMeasure`] the caller supplies. It has
//! no IO and no dependencies, and its own tiny geometry ([`Vec2`],
//! [`Rect`], [`OrientedBox`], [`Cubic`], [`Polyline`]), so it stands alone.
//!
//! ```
//! use lab_arrows::*;
//!
//! // The lab's patch edge: leave toward the pixel, arrive from its left.
//! let arrow = Arrow::new(Vec2::new(0.0, 0.0), Vec2::new(320.0, 90.0))
//!     .arrive(EndDirection::Side(Side::Left))
//!     .label(Label::primary("lamps 0–49 at pixel 0"))
//!     .label(Label::quiet("control.out").near_end());
//! let g = arrow.layout(&AverageCharWidth::default(), &LayoutOptions::default());
//!
//! let d = g.path.to_svg(); // stroke this
//! assert!(d.starts_with("M0,0 C"));
//! assert!(g.heads[0].closed); // fill this
//! for label in &g.labels {
//!     // Fill `frame` with the background, then set the text at `text_at`,
//!     // turned by `angle_deg`, anchored in the middle.
//!     let p = label.placed.expect("both fit");
//!     assert!(p.angle_deg > -90.0 && p.angle_deg <= 90.0);
//! }
//! ```

mod arrow;
mod arrow_geometry;
mod cubic;
mod end_direction;
mod head;
mod head_geometry;
mod label;
mod label_candidates;
mod label_fit;
mod label_geometry;
mod label_role;
mod layout_arrows;
mod layout_options;
mod oriented_box;
mod path;
mod placed_label;
mod placement;
mod polyline;
mod rect;
mod route;
mod side;
mod text_measure;
mod vec2;

pub use arrow::Arrow;
pub use arrow_geometry::ArrowGeometry;
pub use cubic::Cubic;
pub use end_direction::EndDirection;
pub use head::{Head, HeadShape};
pub use head_geometry::{ArrowEnd, HeadGeometry};
pub use label::Label;
pub use label_fit::{DropReason, LabelFit};
pub use label_geometry::LabelGeometry;
pub use label_role::LabelRole;
pub use layout_arrows::layout_arrows;
pub use layout_options::LayoutOptions;
pub use oriented_box::OrientedBox;
pub use path::{Path, Segment};
pub use placed_label::{PlacedLabel, TextAnchor, TextBaseline};
pub use placement::Placement;
pub use polyline::Polyline;
pub use rect::Rect;
pub use route::{Bend, Route};
pub use side::Side;
pub use text_measure::{AverageCharWidth, TextMeasure};
pub use vec2::Vec2;
