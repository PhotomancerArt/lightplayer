//! Roster card surfaces for the 14-state card vocabulary.
//!
//! The vocabulary sheet below is the visual gate for the card grammar; it
//! renders through the SAME card component as the live gallery
//! (`crate::app::home::device_card::DeviceCard`), so sheet and gallery
//! can never drift. M4's runtime pool adds live sim cards to the roster.

#[cfg(feature = "stories")]
pub(crate) mod roster_card_stories;
