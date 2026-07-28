//! Chat-log export affordances: copy-as-markdown and the debug-JSON
//! download.
//!
//! The markdown log is assembled HERE, from the same [`UiAgentView`] DTO
//! the pane renders — a readable scrollback for sharing ("what did the
//! agent do?"). The debug JSON is core-built (the raw model-facing
//! transcript lives in the parked session runtime): the pane dispatches
//! [`UiAgentView::export_debug_action`] and downloads the dump when the
//! DTO's `seq` advances.

use lpa_studio_core::{UiAgentTurn, UiAgentView, UiNoticeLevel};

/// Render the chat as a readable markdown log.
pub(crate) fn chat_markdown(view: &UiAgentView) -> String {
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

/// File name for the debug dump: the shader's file stem, dump-tagged.
pub(crate) fn debug_dump_file_name(view: &UiAgentView) -> String {
    let path = view.artifact.file_path();
    let stem = path
        .as_str()
        .rsplit('/')
        .next()
        .unwrap_or("shader")
        .trim_end_matches(".glsl");
    format!("{stem}-agent-debug.json")
}

/// Copy `text` to the system clipboard (fire-and-forget).
#[cfg(target_arch = "wasm32")]
pub(crate) fn copy_to_clipboard(text: String) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let promise = window.navigator().clipboard().write_text(&text);
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = wasm_bindgen_futures::JsFuture::from(promise).await {
            log::warn!("chat copy failed: {error:?}");
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn copy_to_clipboard(_text: String) {}

/// Hand `json` to the browser as a download (fire-and-forget).
#[cfg(target_arch = "wasm32")]
pub(crate) fn download_json(file_name: &str, json: &str) {
    if let Err(error) = trigger_json_download(file_name, json) {
        log::warn!("debug dump download failed: {error:?}");
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn download_json(_file_name: &str, _json: &str) {}

#[cfg(target_arch = "wasm32")]
fn trigger_json_download(file_name: &str, json: &str) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;

    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(json.as_bytes()).buffer());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/json");
    let blob = web_sys::Blob::new_with_buffer_source_sequence_and_options(&parts, &options)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)?;

    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| wasm_bindgen::JsValue::from_str("no document"))?;
    let anchor: web_sys::HtmlAnchorElement = document.create_element("a")?.unchecked_into();
    anchor.set_href(&url);
    anchor.set_download(file_name);
    anchor.click();
    web_sys::Url::revoke_object_url(&url)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use lpa_studio_core::{UiAgentToolRow, UiNoticeLevel};

    use super::*;

    fn view_with_turns(turns: Vec<UiAgentTurn>) -> UiAgentView {
        let mut view = UiAgentView::empty(
            lpa_studio_core::ArtifactLocation::file("/pulse.glsl"),
            lpa_studio_core::UiAgentAvailability::Ready,
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

    #[test]
    fn dump_file_name_uses_the_shader_stem() {
        let view = view_with_turns(Vec::new());
        assert_eq!(debug_dump_file_name(&view), "pulse-agent-debug.json");
    }
}
