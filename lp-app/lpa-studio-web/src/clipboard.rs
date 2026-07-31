//! The browser clipboard seam.
//!
//! Sharing has no cloud provider yet, so the clipboard and the filesystem
//! are the whole distribution story (see
//! `docs/adr/2026-07-28-share-envelopes.md`). This module is the one place
//! that touches `navigator.clipboard`; core stays sans-IO and deals only in
//! envelope bytes.
//!
//! `navigator.clipboard` is async and permission-gated, and reads in
//! particular are denied outright in some contexts (non-secure origins,
//! unfocused documents, browsers that gate reads behind a user prompt).
//! Both entry points are therefore **best-effort**: they log and give up
//! rather than propagating an error, matching the house style for
//! browser-edge calls (`package_export.rs`, `router.rs`). Callers that need
//! a guaranteed path must offer a manual fallback — a paste box for reads,
//! a selectable text field for writes.

/// Copy `text` to the system clipboard, fire-and-forget.
#[cfg(target_arch = "wasm32")]
pub(crate) fn write_text(text: &str) {
    let Some(clipboard) = web_sys::window().map(|window| window.navigator().clipboard()) else {
        log::warn!("clipboard: no navigator.clipboard to write to");
        return;
    };
    let promise = clipboard.write_text(text);
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(error) = wasm_bindgen_futures::JsFuture::from(promise).await {
            log::warn!("clipboard write rejected: {error:?}");
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn write_text(_text: &str) {}

/// Read the system clipboard and hand the text to `on_text`.
///
/// `on_text` runs only on success: a denied permission, an empty clipboard,
/// or a browser without `readText` logs and drops the callback, so callers
/// should present a manual paste fallback rather than assuming it fires.
#[cfg(target_arch = "wasm32")]
pub(crate) fn read_text(on_text: impl FnOnce(String) + 'static) {
    let Some(clipboard) = web_sys::window().map(|window| window.navigator().clipboard()) else {
        log::warn!("clipboard: no navigator.clipboard to read from");
        return;
    };
    let promise = clipboard.read_text();
    wasm_bindgen_futures::spawn_local(async move {
        match wasm_bindgen_futures::JsFuture::from(promise).await {
            Ok(value) => match value.as_string() {
                Some(text) => on_text(text),
                None => log::warn!("clipboard read returned a non-string"),
            },
            Err(error) => log::warn!("clipboard read rejected: {error:?}"),
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn read_text(_on_text: impl FnOnce(String) + 'static) {}
