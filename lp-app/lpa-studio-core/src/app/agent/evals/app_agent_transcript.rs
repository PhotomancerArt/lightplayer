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
#[serde(tag = "step", rename_all = "snake_case")]
pub(crate) enum EvalStep {
    /// A tool call, as the model emitted it.
    ToolCall { name: String, input: Value },
    /// A scripted answer to the agent's question.
    ScriptedReply { about: String, text: String },
}
