//! Paste a project into the gallery.
//!
//! The natural gesture for a JSON envelope is Cmd-V on the page — you
//! copied it in another tab or a chat window, and the gallery is where
//! projects live. That is a document-level `paste` listener, which means
//! two rules it must never break:
//!
//! 1. **Never swallow a paste aimed at an input.** Renaming a card is a
//!    text field on this very page. The handler ignores any paste whose
//!    target is an editable element.
//! 2. **Never complain about ordinary text.** The listener sees *every*
//!    paste. Anything that is not a well-formed `lp.package` envelope is
//!    dropped silently — [`peek_header`] classifies before anything else
//!    happens.
//!
//! The explicit "Paste" affordance exists because clipboard *reads* are
//! permission-gated and can be denied outright, and because a page that
//! never had focus may not see the event at all.

use dioxus::prelude::*;
use lpa_studio_core::{HOME_NODE_ID, HomeOp, PACKAGE_KIND, UiAction, peek_header};

/// Dispatch a pasted envelope, or return the reason it was not one.
///
/// Split out from the browser plumbing so the classification rules are
/// testable on the host.
pub(crate) fn classify_paste(text: &str) -> Option<HomeOp> {
    let header = peek_header(text).ok()?;
    if header.kind != PACKAGE_KIND {
        return None;
    }
    Some(HomeOp::ImportJson {
        text: text.to_string(),
    })
}

/// Read the clipboard on demand (the explicit affordance).
pub(crate) fn paste_from_clipboard(on_action: EventHandler<UiAction>) {
    crate::clipboard::read_text(move |text| match classify_paste(&text) {
        Some(op) => on_action.call(UiAction::from_op(HOME_NODE_ID, op)),
        None => log::warn!("paste: the clipboard does not hold a LightPlayer project envelope"),
    });
}

/// Install the document-level `paste` listener. Keep the returned guard
/// alive for as long as the gallery is mounted.
#[cfg(target_arch = "wasm32")]
pub(crate) fn install_paste_listener(
    on_action: EventHandler<UiAction>,
) -> Option<std::rc::Rc<PasteListener>> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::closure::Closure;

    let window = web_sys::window()?;
    let document = window.document()?;
    let callback = Closure::<dyn FnMut(web_sys::ClipboardEvent)>::wrap(Box::new(
        move |event: web_sys::ClipboardEvent| {
            if target_is_editable(&event) {
                return;
            }
            let Some(text) = event
                .clipboard_data()
                .and_then(|data| data.get_data("text/plain").ok())
            else {
                return;
            };
            let Some(op) = classify_paste(&text) else {
                // Not ours — leave the paste entirely alone.
                return;
            };
            event.prevent_default();
            on_action.call(UiAction::from_op(HOME_NODE_ID, op));
        },
    ));
    document
        .add_event_listener_with_callback("paste", callback.as_ref().unchecked_ref())
        .ok()?;
    Some(std::rc::Rc::new(PasteListener { document, callback }))
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn install_paste_listener(
    _on_action: EventHandler<UiAction>,
) -> Option<std::rc::Rc<PasteListener>> {
    None
}

/// Whether the paste is aimed at somewhere the user is typing.
#[cfg(target_arch = "wasm32")]
fn target_is_editable(event: &web_sys::ClipboardEvent) -> bool {
    use wasm_bindgen::JsCast;

    let Some(target) = event.target() else {
        return false;
    };
    let Ok(element) = target.dyn_into::<web_sys::HtmlElement>() else {
        return false;
    };
    if element.is_content_editable() {
        return true;
    }
    matches!(
        element.tag_name().to_ascii_uppercase().as_str(),
        "INPUT" | "TEXTAREA" | "SELECT"
    )
}

pub(crate) struct PasteListener {
    #[cfg(target_arch = "wasm32")]
    document: web_sys::Document,
    #[cfg(target_arch = "wasm32")]
    callback: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::ClipboardEvent)>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for PasteListener {
    fn drop(&mut self) {
        use wasm_bindgen::JsCast;
        let _ = self
            .document
            .remove_event_listener_with_callback("paste", self.callback.as_ref().unchecked_ref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(kind: &str) -> String {
        format!(
            r#"{{"kind":"{kind}","format":1,"name":"Demo","files":{{"project.json":{{"text":"{{}}"}}}}}}"#
        )
    }

    #[test]
    fn a_package_envelope_becomes_an_import() {
        let text = envelope("lp.package");
        assert!(matches!(
            classify_paste(&text),
            Some(HomeOp::ImportJson { .. })
        ));
    }

    #[test]
    fn ordinary_pastes_are_ignored_silently() {
        // The listener sees every paste on the page. None of these may
        // produce an action — or a complaint.
        for text in [
            "",
            "hello world",
            "https://example.com",
            r#"{"some":"other json"}"#,
            "{ not json at all",
        ] {
            assert!(classify_paste(text).is_none(), "{text:?}");
        }
    }

    #[test]
    fn a_node_envelope_is_not_a_gallery_paste() {
        // Nodes paste at an attach site inside a project, not here. The
        // gallery must decline rather than mis-install one.
        assert!(classify_paste(&envelope("lp.node")).is_none());
    }

    #[test]
    fn a_future_format_does_not_install() {
        let text = r#"{"kind":"lp.package","format":99,"name":"x","files":{}}"#;
        assert!(classify_paste(text).is_none());
    }
}
