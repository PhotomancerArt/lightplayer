//! The agent chats' shared parts and the app chat's window.
//!
//! Both agent chats — the shader agent's tab (`app::node::agent_chat`) and
//! the app chat — draw through the same parts here: the transcript (tool
//! rows, edit rows, cards as the real control), the error strip, the
//! composer, the footnote and the not-configured state. The app chat adds
//! its drawer (mounted once by the web app, open across navigation), the
//! header button that opens it, and the home page's front door.

pub(crate) mod agent_card_view;
#[cfg(feature = "stories")]
pub(crate) mod agent_card_view_stories;
pub(crate) mod agent_chat_footer;
pub(crate) mod agent_composer;
pub(crate) mod agent_light;
pub(crate) mod agent_needs_key;
pub(crate) mod agent_transcript;
pub(crate) mod app_chat_button;
pub(crate) mod app_chat_context;
pub(crate) mod app_chat_drawer;
pub(crate) mod app_chat_front_door;
pub(crate) mod app_chat_pane;
#[cfg(feature = "stories")]
pub(crate) mod app_chat_stories;

pub(crate) use agent_card_view::AgentCardView;
pub(crate) use agent_chat_footer::AgentChatFooter;
pub(crate) use agent_composer::AgentComposer;
#[cfg(feature = "stories")]
pub(crate) use agent_light::{AgentActivityProvider, story_activity};
pub(crate) use agent_light::{AgentMark, use_agent_reveal, use_provide_agent_activity};
pub(crate) use agent_needs_key::AgentNeedsKey;
pub(crate) use agent_transcript::{AgentErrorStrip, AgentTranscript};
pub(crate) use app_chat_button::AppChatButton;
pub(crate) use app_chat_context::{use_app_chat_chrome, use_provide_app_chat_chrome};
pub(crate) use app_chat_drawer::AppChatDrawer;
pub(crate) use app_chat_front_door::AppChatFrontDoor;
