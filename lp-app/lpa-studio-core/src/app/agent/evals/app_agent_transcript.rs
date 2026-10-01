//! [`EvalTranscript`]: the conversation an eval run had, in the order it
//! happened — what the transcript checks read and what lands in
//! `transcript.json`. Never carries a provider config, so never a key.

use serde_json::Value;

/// The run's conversation, flattened.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub(crate) struct EvalTranscript {
    pub(crate) steps: Vec<EvalStep>,
}

/// One thing that happened.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvalStep {
    /// A user message (the scenario's, or a scripted reply as sent).
    User { text: String },
    /// The `<app_state>` block that rode a message.
    State { text: String },
    /// The agent's visible text for one model turn.
    Assistant { text: String },
    /// A tool call, as the model emitted it.
    ToolCall { name: String, input: Value },
    /// The tool's result, as the model saw it.
    ToolResult { name: String, content: String },
    /// A scripted answer to the agent's question (recorded where it was
    /// given, so ordering checks can see what came before it).
    ScriptedReply { about: String, text: String },
    /// Why the scenario stopped early (budget, an unanswered question, a
    /// provider error).
    Stopped { reason: String },
}
