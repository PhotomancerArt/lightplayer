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
    /// The agent ended a turn on a question: its tail, and how many things
    /// it asked at once (the `?`s in it).
    Question { tail: String, questions: usize },
    /// The persona's `otherwise` answer to a question nobody scripted —
    /// an *unscripted question* in the report.
    FallbackReply { question: String, text: String },
    /// A follow-up message (`then`), sent after a turn that ended without
    /// a question.
    FollowUp { text: String },
    /// The agent handed over a card (recorded when it first appears).
    CardHanded {
        card: String,
        offer: Option<String>,
        title: String,
        destructive: bool,
    },
    /// The person clicked a card, with the values they picked over the
    /// agent's.
    CardClicked {
        card: String,
        offer: Option<String>,
        args: std::collections::BTreeMap<String, String>,
    },
    /// The person left a card alone.
    CardLeft { card: String, offer: Option<String> },
    /// Why the scenario stopped early (budget, an unanswered question, a
    /// provider error).
    Stopped { reason: String },
    /// A notice the chat showed the user (a truncated turn, a provider
    /// error, the turn limit) — what the model-facing transcript cannot
    /// carry, because a torn tool call never reaches it.
    Notice { text: String },
}
