//! The app agent's surfaces: the cards it hands the user in the chat.
//!
//! The app chat's own window is the agentic-UI roadmap's M5; until then the
//! one chat transcript renderer (`app::node::agent_chat`) draws a card
//! wherever a transcript holds one.

pub(crate) mod agent_card_view;
#[cfg(feature = "stories")]
pub(crate) mod agent_card_view_stories;

pub(crate) use agent_card_view::AgentCardView;
