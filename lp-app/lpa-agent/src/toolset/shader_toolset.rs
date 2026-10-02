//! [`ShaderToolset`]: the shader agent's three tools over an
//! [`AgentHost`], exactly as the session ran them before the seam.

use lps_probe::CompiledShader;
use serde_json::{Value, json};

use crate::prompt::system_prompt::build_system_prompt;
use crate::provider::model_provider::ToolDef;
use crate::tool::declare_space_tool::{
    DECLARE_SPACE_TOOL_NAME, declare_space_tool_def, run_declare_space,
};
use crate::tool::iterate_host::{AgentHost, HostFuture};
use crate::tool::iterate_tool::{ITERATE_TOOL_NAME, iterate_tool_def, run_iterate};
use crate::tool::tool_phase::ToolPhase;
use crate::tool::upsert_param_tool::{
    UPSERT_PARAM_TOOL_NAME, run_upsert_param, upsert_param_tool_def,
};
use crate::toolset::tool_outcome::ToolOutcome;
use crate::toolset::toolset::Toolset;

/// `iterate` + `upsert_param` + `declare_space` over one shader's host.
pub struct ShaderToolset<H: AgentHost> {
    host: H,
    /// Last successfully compiled shader (the `iterate` diff cache).
    prev_compiled: Option<CompiledShader>,
}

impl<H: AgentHost> ShaderToolset<H> {
    pub fn new(host: H) -> Self {
        Self {
            host,
            prev_compiled: None,
        }
    }

    pub fn host(&self) -> &H {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }
}

impl<H: AgentHost> Toolset for ShaderToolset<H> {
    fn tool_defs(&self) -> Vec<ToolDef> {
        vec![
            iterate_tool_def(),
            upsert_param_tool_def(),
            declare_space_tool_def(),
        ]
    }

    /// Rebuilt every turn: staged edits change the current source.
    fn system_prompt(&self) -> String {
        let current_source = self.host.current_source().unwrap_or_default();
        build_system_prompt(&self.host.shader_context(), &current_source)
    }

    fn run_tool<'a>(
        &'a mut self,
        name: &'a str,
        input: &'a Value,
        progress: &'a mut dyn FnMut(ToolPhase),
    ) -> HostFuture<'a, ToolOutcome> {
        Box::pin(async move {
            if name == ITERATE_TOOL_NAME {
                run_iterate(input, &mut self.host, &mut self.prev_compiled, progress).await
            } else if name == UPSERT_PARAM_TOOL_NAME {
                run_upsert_param(input, &mut self.host, progress).await
            } else if name == DECLARE_SPACE_TOOL_NAME {
                run_declare_space(input, &mut self.host, progress).await
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
