//! Export-to-zip download for gallery cards.
//!
//! Export reads the mounted library directly (it's read-only), encodes with
//! the M3 zip codec, and hands the bytes to the browser as a download —
//! no actor round-trip.

use lpa_studio_core::UiPackageCard;

#[cfg(target_arch = "wasm32")]
pub(crate) fn export_package_to_download(card: &UiPackageCard) {
    use lpa_studio_core::app::library::export_package;

    let Some(store) = crate::local_store::library_store() else {
        log::warn!("export: the local library is unavailable");
        return;
    };
    let uid = match card.uid.parse() {
        Ok(uid) => uid,
        Err(error) => {
            log::warn!("export: invalid package uid {}: {error}", card.uid);
            return;
        }
    };
    let bytes = match store.open(uid).and_then(|handle| export_package(&handle)) {
        Ok(bytes) => bytes,
        Err(error) => {
            log::warn!("export of {} failed: {error}", card.name);
            return;
        }
    };
    if let Err(error) = trigger_download(&export_file_name(&card.slug), &bytes) {
        log::warn!("export download of {} failed: {error:?}", card.name);
    }
}

/// `2026-07-08-1851-porch-sign.zip` — local wall-clock date and time so a
/// folder of exports reads chronologically.
#[cfg(target_arch = "wasm32")]
fn export_file_name(slug: &str) -> String {
    let now = js_sys::Date::new_0();
    format!(
        "{:04}-{:02}-{:02}-{:02}{:02}-{slug}.zip",
        now.get_full_year(),
        now.get_month() + 1,
        now.get_date(),
        now.get_hours(),
        now.get_minutes(),
    )
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
