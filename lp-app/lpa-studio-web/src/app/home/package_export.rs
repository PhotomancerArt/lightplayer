//! Export-to-zip download for gallery cards.
//!
//! Export hydrates a lock-free library snapshot through the host
//! (read-only by design), encodes with the M3 zip codec, and hands the
//! bytes to the browser as a download — no actor round-trip.

use lpa_studio_core::UiPackageCard;

#[cfg(target_arch = "wasm32")]
pub(crate) fn export_package_to_download(card: &UiPackageCard) {
    use lpa_studio_core::app::library::{LibraryStore, export_package};

    let Some(host) = crate::local_store::library_host() else {
        log::warn!("export: the local library is unavailable");
        return;
    };
    let uid_key = card.uid.clone();
    let slug = card.slug.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let fs = match host.catalog_snapshot().await {
            Ok(fs) => fs,
            Err(error) => {
                log::warn!("export snapshot failed: {error}");
                return;
            }
        };
        let store = LibraryStore::read_only(fs);
        let uid = match store.resolve_key(&uid_key) {
            Ok(uid) => uid,
            Err(error) => {
                log::warn!("export: cannot resolve {uid_key}: {error}");
                return;
            }
        };
        let bytes = match store.open(uid).and_then(|handle| export_package(&handle)) {
            Ok(bytes) => bytes,
            Err(error) => {
                log::warn!("export of {slug} failed: {error}");
                return;
            }
        };
        // the slug already carries its date stamp — no extra prefix
        if let Err(error) = trigger_download(&format!("{slug}.zip"), &bytes) {
            log::warn!("export download of {slug} failed: {error:?}");
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn export_package_to_download(_card: &UiPackageCard) {}

#[cfg(target_arch = "wasm32")]
fn trigger_download(file_name: &str, bytes: &[u8]) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast;

    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes).buffer());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/zip");
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
