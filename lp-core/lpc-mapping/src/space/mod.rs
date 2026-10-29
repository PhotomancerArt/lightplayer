//! Fixture and shader space: for one fixture and one source, which
//! coordinate does the source see at a lamp, or is the lamp dark?
//!
//! The theory (planning `lp2025/2026-10-28-0935-fixture-space-theory`,
//! `theory.md` D3–D28; the ADR "Fixture and shader space" records it):
//!
//! - A fixture's box is its map2d `view_box` ([`ViewBox`], SVG semantics,
//!   y down). A lamp outside it is dark (D12, D14).
//! - A source's box is a [`ProductBox`]: a **view** (unbounded, a shader;
//!   the box is a suggestion) or an **extent** (bounded: nothing outside).
//!   The interfaces' views are [`ProductBox::unit_2d`] (−1…1, y up) and
//!   [`ProductBox::shadertoy_2d`] (`fragCoord`, 0…N, y up).
//! - The fixture authors a [`Fit`] (contain, cover, fill; nine alignments;
//!   scale and rotation about the fitted product's centre, D16). From the
//!   fit, the fixture's box and the source's box comes **one affine T**
//!   ([`SpaceTransform`]) from fixture space to source space; the y flip
//!   between a y-down fixture and a y-up interface lives inside it (D16).
//! - A sample past a bounded extent follows the fixture's [`OutsideRule`]
//!   (solid, clamp, wrap, mirror, per axis — GPU sampler address modes,
//!   D6, D18).
//! - `unit`'s uniforms come through T ([`UnitScope`], D22); a 1D source lands
//!   as the fixture says ([`Landing1d`], D23–D25), and along the order a
//!   lamp's position is `k/(N−1)` ([`order_position_q16`], D27).
//!
//! T is applied as an exact fixed-point affine (pattern space's technique,
//! `docs/adr/2026-09-24-pattern-space.md`): built once in `f64` when the
//! source or the mapping changes, applied per lamp in integer arithmetic,
//! Q16.16 out, rounded to nearest and saturated. See [`SpaceTransform`].
//!
//! Pure and `no_std + alloc`: nothing here reads a clock, a file or the
//! engine. The engine, Studio and the upgrade share it.

mod address_mode;
mod fit;
mod fit_align;
mod fit_mode;
mod landing_1d;
mod order_position;
mod outside_rule;
mod product_box;
mod sample;
mod space_error;
mod space_transform;
mod unit_scope;
mod view_box;

pub use address_mode::AddressMode;
pub use fit::Fit;
pub use fit_align::{AlignAxis, FitAlign};
pub use fit_mode::FitMode;
pub use landing_1d::{DEFAULT_LANDING_1D, Landing1d, LandingFlip, LandingMirror, LandingShape};
pub use order_position::order_position_q16;
pub use outside_rule::{DEFAULT_OUTSIDE_RULE, OutsideRule};
pub use product_box::{ProductBox, ProductBoxKind};
pub use sample::Sample;
pub use space_error::SpaceError;
pub use space_transform::{Q16_ONE, REL_ONE, SpaceTransform};
pub use unit_scope::UnitScope;
pub use view_box::ViewBox;
