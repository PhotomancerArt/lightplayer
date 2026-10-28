//! Edits to a [`Mapping`](crate::part::Mapping), with the model's rules for
//! the ones that rewrite references (plan D6, D12).
//!
//! Most edits are field sets. Some are not: an override is keyed by a
//! node's id, so renaming a part, wrapping it in a repeat, expanding a
//! repeat or dropping copies changes which ids exist. Those rules are the
//! model's, not an editor's, so they live here and every editor (Studio's
//! Graph, the app agent through its offers, `lp-cli` someday) gets the same
//! answers.
//!
//! **Overrides are never retargeted** (D12). An edit either carries an
//! override to the node it was on (under that node's new id), or keeps it
//! as it was, for the patcher to report as gone ([`Problem::GoneNode`]). An
//! edit that could keep one only by letting it name a different node is
//! refused ([`EditError::WouldRetarget`]).
//!
//! Every operation takes `&mut Mapping` and returns `Result<_, EditError>`.
//! A refused edit changes nothing.
//!
//! Compiled only with the `edit` cargo feature: no firmware enables it, so
//! none of this is in a device image.
//!
//! [`Problem::GoneNode`]: crate::patching::Problem::GoneNode

mod add_part;
mod add_ring;
mod clear_override_field;
mod copy_levels;
mod edit_error;
mod expand_repeat;
mod free_id;
mod insert_part;
mod move_part;
mod override_field;
mod part_place;
mod remove_part;
mod remove_ring;
mod rename_part;
mod rotate_part;
mod set_circle_split;
mod set_drawn_from;
mod set_flow;
mod set_lamp_count;
mod set_override_field;
mod set_path_split;
mod set_repeat_count;
mod set_start;
mod unrepeat;
mod wrap_in_repeat;

pub use add_part::add_part;
pub use add_ring::add_ring;
pub use clear_override_field::clear_override_field;
pub use edit_error::EditError;
pub use expand_repeat::expand_repeat;
pub use insert_part::insert_part;
pub use move_part::move_part;
pub use override_field::{OverrideField, OverrideFieldKind};
pub use remove_part::{RemovedPart, remove_part};
pub use remove_ring::remove_ring;
pub use rename_part::rename_part;
pub use set_circle_split::set_circle_split;
pub use set_drawn_from::{DrawnFrom, set_drawn_from};
pub use set_flow::set_flow;
pub use set_lamp_count::set_lamp_count;
pub use set_override_field::set_override_field;
pub use set_path_split::set_path_split;
pub use set_repeat_count::{overrides_past_count, set_repeat_count};
pub use set_start::set_start;
pub use unrepeat::unrepeat;
pub use wrap_in_repeat::wrap_in_repeat;
