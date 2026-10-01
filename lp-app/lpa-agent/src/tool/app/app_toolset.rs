//! [`AppToolset`]: the app agent's tools, static prompt and per-turn
//! state, over an [`AppAgentHost`].

use serde_json::{Value, json};

use crate::tool::app::edit_project_tool::{
    EDIT_PROJECT_TOOL_NAME, edit_project_tool_def, run_edit_project,
};
use crate::tool::app::read_tool::{READ_TOOL_NAME, read_tool_def, run_read};

use crate::prompt::app::build_app_system_prompt;
use crate::provider::model_provider::ToolDef;
use crate::tool::app::app_agent_host::AppAgentHost;
use crate::tool::iterate_host::HostFuture;
use crate::tool::tool_phase::ToolPhase;
use crate::toolset::{ToolOutcome, Toolset};

/// The app agent's toolset. The system prompt is built once, at
/// construction, from the host's generated reference, and never changes
/// for the session (PD3).
pub struct AppToolset<H: AppAgentHost> {
    host: H,
    system: String,
}

impl<H: AppAgentHost> AppToolset<H> {
    pub fn new(host: H, reference: &str) -> Self {
        Self {
            host,
            system: build_app_system_prompt(reference),
        }
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }
}

impl<H: AppAgentHost> Toolset for AppToolset<H> {
    fn tool_defs(&self) -> Vec<ToolDef> {
        vec![edit_project_tool_def(), read_tool_def()]
    }

    fn system_prompt(&self) -> String {
        self.system.clone()
    }

    /// Open models write malformed JSON on long nested `edit_project`
    /// arguments; answering in-band lets them resend (D13).
    fn answers_malformed_tool_input(&self) -> bool {
        true
    }

    fn turn_state(&mut self) -> Option<String> {
        Some(self.host.readout())
    }

    fn run_tool<'a>(
        &'a mut self,
        name: &'a str,
        input: &'a Value,
        _progress: &'a mut dyn FnMut(ToolPhase),
    ) -> HostFuture<'a, ToolOutcome> {
        Box::pin(async move {
            if name == EDIT_PROJECT_TOOL_NAME {
                run_edit_project(input, &mut self.host).await
            } else if name == READ_TOOL_NAME {
                run_read(input, &mut self.host).await
            } else {
                ToolOutcome {
                    content: json!({ "error": format!("unknown tool {name:?}") }).to_string(),
                    is_error: true,
                    summary: json!({ "error": "unknown tool" }),
                }
            }
        })
    }
}
