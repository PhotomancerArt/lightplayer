//! The rich-object pattern: **pane + status + detail-with-sections +
//! content**, one derivation shared by every surface.
//!
//! A rich object (a node, a device, later a runtime) publishes an ordered
//! list of [`RichSection`]s in a fixed schema order; the object-level
//! rollup ([`RichObjectView::rollup`]) derives the ONE indicator tone and
//! the ONE primary affordance every surface consumes — cards, detail
//! popovers, and pane headers are renderers over the same model, never
//! re-derivers. Decision record:
//! `docs/adr/2026-07-17-rich-object-pattern.md` (builds on the 2026-07-16
//! card-vocabulary ADR).
//!
//! Concept map:
//! - [`rich_section`]: one section — title, tone, fact lines, advisory
//!   chip, affordance identities, rollup weight.
//! - [`rich_object_view`]: the ordered sections + the two rollup rules
//!   (worst ACTIONABLE section's tone = the indicator; that section's
//!   affordance = the primary affordance; Advisory/Danger never roll up).
//!
//! The device builder lives with its evidence in
//! [`crate::app::roster::device_rich_object`].

pub mod rich_object_view;
pub mod rich_section;

pub use rich_object_view::{RichObjectView, RichRollup};
pub use rich_section::{RichChip, RichLine, RichSection, RichWeight};
