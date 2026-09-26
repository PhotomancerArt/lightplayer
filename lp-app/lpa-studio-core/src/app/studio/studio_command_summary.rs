//! What the session recorder says about a [`StudioCommand`] (and a
//! [`UiAction`]): a stable name and a bounded detail line.
//!
//! The recorder (`?record=`) wants every user action in its timeline, but a
//! command is not a thing to dump: an import carries whole package files as
//! bytes, a settings command can carry an API key, and an access command a
//! password. So:
//!
//! - `name` is the variant path — `Action/home/OpenPackage`,
//!   `Device/Action/AddFromUsb`, `Settings/SetAgentAnthropicApiKey` — which
//!   is what a timeline is read by;
//! - `detail` is the `Debug` rendering cut at [`COMMAND_DETAIL_LIMIT`]
//!   characters (the formatter stops writing there, so a large payload is
//!   never rendered in full just to be thrown away), and EMPTY for the
//!   commands that can carry a secret (settings, access);
//! - the chatty inputs that already have their own record are skipped:
//!   refresh ticks (a timer, not an action), device link events (mirrored
//!   one for one by the device journal), and streamed agent events.

use core::fmt::{self, Write as _};

use crate::UiAction;
use crate::app::agent::AgentFeedback;
use crate::app::studio::studio_command::StudioCommand;

/// The longest `detail` the recorder keeps, in characters.
pub const COMMAND_DETAIL_LIMIT: usize = 300;

/// The recorder's `(name, detail)` for a command, or `None` for a command
/// the recorder skips (see the module doc).
pub fn summarize_command(command: &StudioCommand) -> Option<(String, String)> {
    let summary = match command {
        StudioCommand::RefreshTick => return None,
        StudioCommand::Device(lpa_devices::Input::Event(_)) => return None,
        StudioCommand::Agent(AgentFeedback::Event { .. }) => return None,
        StudioCommand::Action(action) => (
            format!("Action/{}", action_name(action)),
            bounded_debug(action_op_debug(action)),
        ),
        StudioCommand::Device(lpa_devices::Input::Action(action)) => (
            format!("Device/Action/{}", variant_of(&bounded_debug(action))),
            bounded_debug(action),
        ),
        StudioCommand::DeviceHotplug(edge) => (format!("DeviceHotplug/{edge:?}"), String::new()),
        StudioCommand::Console(console) => (
            format!("Console/{}", variant_of(&bounded_debug(console))),
            bounded_debug(console),
        ),
        // Settings and access commands can carry an API key or a
        // password: the name only.
        StudioCommand::Settings(settings) => (
            format!("Settings/{}", variant_of(&bounded_debug(settings))),
            String::new(),
        ),
        StudioCommand::Access(access) => (
            format!("Access/{}", variant_of(&bounded_debug(access))),
            String::new(),
        ),
        StudioCommand::Agent(feedback) => (
            format!("Agent/{}", variant_of(&bounded_debug(feedback))),
            bounded_debug(feedback),
        ),
        StudioCommand::AttachLibrary(_) => ("AttachLibrary".to_string(), String::new()),
        StudioCommand::PageVisibility { visible } => {
            ("PageVisibility".to_string(), format!("visible={visible}"))
        }
        StudioCommand::LibraryChanged => ("LibraryChanged".to_string(), String::new()),
        StudioCommand::Shutdown => ("Shutdown".to_string(), String::new()),
    };
    Some(summary)
}

/// An action's recorder name: its controller id and its op's variant
/// (`home/OpenPackage`).
pub fn action_name(action: &UiAction) -> String {
    format!(
        "{}/{}",
        action.node_id().as_str(),
        variant_of(&bounded_debug(action_op_debug(action)))
    )
}

/// The action's op as a `Debug` value (the op, not the render metadata the
/// `UiAction` also carries).
fn action_op_debug(action: &UiAction) -> impl fmt::Debug + '_ {
    OpDebug(action)
}

struct OpDebug<'a>(&'a UiAction);

impl fmt::Debug for OpDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt_op(f)
    }
}

/// `value`'s `Debug` rendering, cut at [`COMMAND_DETAIL_LIMIT`] characters
/// with a trailing `…` when it was longer.
pub fn bounded_debug(value: impl fmt::Debug) -> String {
    let mut out = BoundedWriter {
        text: String::new(),
        chars: 0,
        truncated: false,
    };
    // An `Err` here is the writer refusing past the limit — expected.
    let _ = write!(out, "{value:?}");
    if out.truncated {
        out.text.push('…');
    }
    out.text
}

/// The leading identifier of a `Debug` rendering — its variant or type
/// name (`OpenPackage { .. }` → `OpenPackage`).
fn variant_of(debug: &str) -> String {
    debug
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// A `fmt::Write` that stops (with an error, which ends the formatting)
/// once it holds [`COMMAND_DETAIL_LIMIT`] characters.
struct BoundedWriter {
    text: String,
    chars: usize,
    truncated: bool,
}

impl fmt::Write for BoundedWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            if self.chars == COMMAND_DETAIL_LIMIT {
                self.truncated = true;
                return Err(fmt::Error);
            }
            self.text.push(c);
            self.chars += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::settings::SettingsCommand;
    use crate::{ControllerId, HOME_NODE_ID, HomeOp};

    #[test]
    fn an_action_is_named_by_its_node_and_op_variant() {
        let action = UiAction::from_op(
            ControllerId::new(HOME_NODE_ID),
            HomeOp::OpenPackage {
                key: "pkg-1".to_string(),
                prefer: None,
            },
        );
        let (name, detail) = summarize_command(&StudioCommand::Action(action)).unwrap();
        assert_eq!(name, format!("Action/{HOME_NODE_ID}/OpenPackage"));
        assert!(detail.starts_with("OpenPackage {"), "{detail}");
        assert!(detail.contains("pkg-1"), "{detail}");
    }

    #[test]
    fn ticks_and_device_link_events_are_skipped() {
        assert!(summarize_command(&StudioCommand::RefreshTick).is_none());
    }

    #[test]
    fn settings_commands_never_carry_their_value() {
        let command = StudioCommand::Settings(SettingsCommand::SetAgentAnthropicApiKey(Some(
            "sk-secret".to_string(),
        )));
        let (name, detail) = summarize_command(&command).unwrap();
        assert_eq!(name, "Settings/SetAgentAnthropicApiKey");
        assert!(detail.is_empty());
    }

    #[test]
    fn a_large_payload_is_cut_at_the_limit() {
        let big = vec![7u8; 10_000];
        let detail = bounded_debug(&big);
        assert_eq!(detail.chars().count(), COMMAND_DETAIL_LIMIT + 1);
        assert!(detail.ends_with('…'));
        assert_eq!(bounded_debug("short"), "\"short\"");
    }

    #[test]
    fn page_visibility_says_which_way() {
        let (name, detail) =
            summarize_command(&StudioCommand::PageVisibility { visible: false }).unwrap();
        assert_eq!(name, "PageVisibility");
        assert_eq!(detail, "visible=false");
    }
}
