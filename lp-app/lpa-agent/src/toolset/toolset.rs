//! [`Toolset`]: the tools, the system prompt and the per-turn state one
//! kind of agent offers its model.

use serde_json::Value;

use crate::provider::model_provider::ToolDef;
use crate::tool::iterate_host::HostFuture;
use crate::tool::tool_phase::ToolPhase;
use crate::toolset::tool_outcome::ToolOutcome;

/// Opening fence of the volatile state block a toolset's
/// [`Toolset::turn_state`] rides in (PD3: changing state lives in the
/// message stream, never in the system prompt).
pub const APP_STATE_OPEN: &str = "<app_state>";
/// Closing fence of the state block.
pub const APP_STATE_CLOSE: &str = "</app_state>";

/// What a session offers the model, and how it runs a call.
///
/// Implementations are `!Send` and runtime-neutral like the rest of the
/// crate: [`Toolset::run_tool`] returns a boxed future the session awaits
/// inline.
pub trait Toolset {
    /// The tools offered on every turn.
    fn tool_defs(&self) -> Vec<ToolDef>;

    /// The system prompt for the next model turn. The shader toolset
    /// rebuilds it each turn (staged edits change the source it embeds);
    /// a toolset with [`Toolset::turn_state`] keeps it byte-stable so a
    /// provider cache can hold the prefix.
    fn system_prompt(&self) -> String;

    /// Volatile state to send with the next user turn, and again after
    /// each tool round (fenced by [`wrap_turn_state`]). `None` sends
    /// nothing.
    fn turn_state(&mut self) -> Option<String> {
        None
    }

    /// Run one tool call. An unknown tool or a malformed input is an
    /// in-band `{"error": …}` result, never a panic.
    fn run_tool<'a>(
        &'a mut self,
        name: &'a str,
        input: &'a Value,
        progress: &'a mut dyn FnMut(ToolPhase),
    ) -> HostFuture<'a, ToolOutcome>;
}

/// The state block as it rides a message.
pub fn wrap_turn_state(state: &str) -> String {
    format!("{APP_STATE_OPEN}\n{state}\n{APP_STATE_CLOSE}")
}
