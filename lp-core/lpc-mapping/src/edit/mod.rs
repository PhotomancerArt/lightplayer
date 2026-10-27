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

mod copy_levels;
mod edit_error;
mod expand_repeat;
mod free_id;
mod insert_part;
mod move_part;
mod part_place;
mod remove_part;
mod rename_part;
mod rotate_part;
mod set_repeat_count;
mod unrepeat;
mod wrap_in_repeat;

pub use edit_error::EditError;
pub use expand_repeat::expand_repeat;
pub use insert_part::insert_part;
pub use move_part::move_part;
pub use remove_part::{RemovedPart, remove_part};
pub use rename_part::rename_part;
pub use set_repeat_count::set_repeat_count;
pub use unrepeat::unrepeat;
pub use wrap_in_repeat::wrap_in_repeat;
