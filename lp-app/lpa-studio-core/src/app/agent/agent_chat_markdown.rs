//! Render one agent chat as a readable markdown log.
//!
//! Pure over the [`UiAgentView`] DTO, so both shells share it: the web
//! pane's copy-as-markdown affordance and the headless runner's `run.md`
//! artifact (P2). Moved here from `lpa-studio-web::agent_chat_export`
//! verbatim — the web crate re-exports it.

use crate::UiNoticeLevel;
use crate::app::agent::ui_agent_view::{UiAgentTurn, UiAgentView};

/// Render the chat as a readable markdown log.
pub fn chat_markdown(view: &UiAgentView) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Shader agent chat — {}\n",
        view.artifact.file_path().as_str()
    ));
    if let Some(model) = view.model.effective.as_deref() {
        out.push_str(&format!("Model: {model}\n"));
    }
    out.push('\n');
    for turn in &view.turns {
        match turn {
            UiAgentTurn::User { text } => {
                out.push_str(&format!("## User\n\n{text}\n\n"));
            }
            UiAgentTurn::Assistant { text } => {
                out.push_str(&format!("## Agent\n\n{text}\n\n"));
            }
            UiAgentTurn::Thinking { text, .. } => {
                out.push_str(&format!("> *Thinking:* {}\n\n", text.replace('\n', " ")));
            }
            UiAgentTurn::Tool(row) => {
                let note = row.note.as_deref().unwrap_or("(no note)");
                out.push_str(&format!("**Tool** — {note}\n"));
                if let Some(shader_ok) = row.shader_ok {
                    out.push_str(&format!(
                        "- probe compile {}, {} probes, {} warnings{}\n",
                        if shader_ok { "ok" } else { "failed" },
                        row.probes,
                        row.warnings,
                        if row.staged { ", staged an edit" } else { "" },
                    ));
                }
                // The ENGINE verdict is a different compile world than the
                // probe harness (backend codegen can reject what probes
                // accept) — say both, or the log reads "ok" while the
                // engine is failing.
                if let Some(entry) = row
                    .edit_turn
                    .and_then(|turn| view.history.iter().find(|entry| entry.turn == turn))
                {
                    out.push_str(match entry.engine_ok {
                        Some(true) => "- engine: ok\n",
                        Some(false) => "- engine: ERROR (backend rejected the staged source)\n",
                        None => "- engine: unresolved\n",
                    });
                }
                if let Some(error) = &row.error {
                    out.push_str(&format!("- error: {error}\n"));
                }
                out.push('\n');
            }
            UiAgentTurn::Notice { text, level } => {
                let tag = match level {
                    UiNoticeLevel::Warning => "⚠ ",
                    _ => "",
                };
                out.push_str(&format!("*{tag}{text}*\n\n"));
            }
        }
    }
    if !view.usage.is_zero() {
        out.push_str(&format!(
            "---\n{} in · {} out tokens",
            view.usage.total_input_tokens(),
            view.usage.output_tokens
        ));
        // Cache buckets, spelled out: "no cache hits" on a long session is
        // a prompt-caching regression signal, not a detail to hide.
        if view.usage.cache_read_tokens > 0 {
            out.push_str(&format!(
                " · {} cached reads, {} cache writes",
                view.usage.cache_read_tokens, view.usage.cache_write_tokens
            ));
        } else {
            out.push_str(" · NO CACHE HITS");
        }
        if let Some(cost) = view.estimated_cost.as_deref() {
            out.push_str(&format!(" · {cost}"));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use lpc_model::ArtifactLocation;

    use super::*;
    use crate::{UiAgentAvailability, UiAgentToolRow};

    fn view_with_turns(turns: Vec<UiAgentTurn>) -> UiAgentView {
        let mut view = UiAgentView::empty(
            ArtifactLocation::file("/pulse.glsl"),
            UiAgentAvailability::Ready,
        );
        view.turns = turns;
        view
    }

    #[test]
    fn markdown_renders_every_turn_kind_in_order() {
        let markdown = chat_markdown(&view_with_turns(vec![
            UiAgentTurn::User {
                text: "make it pulse".into(),
            },
            UiAgentTurn::Thinking {
                text: "plan:\nslow it".into(),
                done: true,
            },
            UiAgentTurn::Tool(UiAgentToolRow {
                note: Some("slow the pulse".into()),
                ..UiAgentToolRow::started("tu_1")
            }),
            UiAgentTurn::Assistant {
                text: "Done.".into(),
            },
            UiAgentTurn::Notice {
                text: "Run stopped: the response hit the output-token limit.".into(),
                level: UiNoticeLevel::Warning,
            },
        ]));
        let expected_order = [
            "## User",
            "make it pulse",
            "*Thinking:* plan: slow it",
            "**Tool** — slow the pulse",
            "## Agent",
            "⚠ Run stopped",
        ];
        let mut cursor = 0;
        for needle in expected_order {
            let at = markdown[cursor..].find(needle).unwrap_or_else(|| {
                panic!("missing {needle:?} after byte {cursor} in:\n{markdown}")
            });
            cursor += at + needle.len();
        }
    }
}
