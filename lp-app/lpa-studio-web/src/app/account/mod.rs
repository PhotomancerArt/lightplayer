//! The `/account` surface: your profile, your account facts, your sessions.
//!
//! One page, three groups (spike `cloud-login` §3 — visual reference only,
//! never imported). It is deliberately the *whole* account surface: the
//! AI-token balance and the published-projects list get groups here later
//! without a redesign, which is exactly why the settings-rows layout won
//! over the narrow identity card.

pub mod account_page;
#[cfg(feature = "stories")]
pub(crate) mod account_page_stories;

pub use account_page::AccountPage;
