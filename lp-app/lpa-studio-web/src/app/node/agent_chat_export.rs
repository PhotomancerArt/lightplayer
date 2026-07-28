//! Chat-log export affordances: copy-as-markdown and the debug-JSON
//! download.
//!
//! The markdown log is built by core's pure `chat_markdown` (moved there so
//! the headless runner's `run.md` artifact shares it) from the same
//! [`UiAgentView`] DTO the pane renders — a readable scrollback for
//! sharing ("what did the agent do?"). The debug JSON is core-built (the
//! raw model-facing transcript lives in the parked session runtime): the
//! pane dispatches [`UiAgentView::export_debug_action`] and downloads the
//! dump when the DTO's `seq` advances.

use lpa_studio_core::UiAgentView;

pub(crate) use lpa_studio_core::chat_markdown;

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
    use super::*;

    #[test]
    fn dump_file_name_uses_the_shader_stem() {
        let view = UiAgentView::empty(
            lpa_studio_core::ArtifactLocation::file("/pulse.glsl"),
            lpa_studio_core::UiAgentAvailability::Ready,
        );
        assert_eq!(debug_dump_file_name(&view), "pulse-agent-debug.json");
    }
}
