//! Export a library package to the clipboard or a download.
//!
//! Both forms hydrate a lock-free library snapshot through the host
//! (read-only by design) and encode from the same file list — the M3 zip
//! codec for the download, the `lp.package` share envelope for the
//! clipboard (`docs/adr/2026-07-28-share-envelopes.md`). No actor
//! round-trip either way.
//!
//! **These read the SAVED bytes.** The library snapshot is what is on
//! disk; unsaved overlay edits are not in it. Callers reachable while a
//! project is dirty (the editor's project popup) must save first — see
//! `ProjectShareSection`. The gallery's own cards can only be dirty for
//! the project currently open in the editor, and that card is not the
//! export surface.

use lpa_studio_core::UiPackageCard;

/// What a package is being exported as.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExportForm {
    /// A `.zip` download.
    Zip,
    /// An `lp.package` envelope on the clipboard.
    JsonToClipboard,
}

/// Identify a package for export. The card carries both fields already;
/// the editor popup supplies them from the open project.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExportTarget {
    /// `prj…` uid or slug — anything `LibraryStore::resolve_key` takes.
    pub uid: String,
    /// Human-facing slug, used for the download filename.
    pub slug: String,
}

impl From<&UiPackageCard> for ExportTarget {
    fn from(card: &UiPackageCard) -> Self {
        Self {
            uid: card.uid.clone(),
            slug: card.slug.clone(),
        }
    }
}

/// Export a package. Best-effort: failures log and give up, matching the
/// house style for browser-edge work.
#[cfg(target_arch = "wasm32")]
pub(crate) fn export_package_as(target: ExportTarget, form: ExportForm) {
    use lpa_studio_core::PackageEnvelope;
    use lpa_studio_core::app::library::{LibraryStore, export_package};

    let Some(host) = crate::local_store::library_host() else {
        log::warn!("export: the local library is unavailable");
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        let fs = match host.catalog_snapshot().await {
            Ok(fs) => fs,
            Err(error) => {
                log::warn!("export snapshot failed: {error}");
                return;
            }
        };
        let store = LibraryStore::read_only(fs);
        let uid = match store.resolve_key(&target.uid) {
            Ok(uid) => uid,
            Err(error) => {
                log::warn!("export: cannot resolve {}: {error}", target.uid);
                return;
            }
        };
        let handle = match store.open(uid) {
            Ok(handle) => handle,
            Err(error) => {
                log::warn!("export of {} failed: {error}", target.slug);
                return;
            }
        };
        match form {
            ExportForm::Zip => {
                let bytes = match export_package(&handle) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        log::warn!("export of {} failed: {error}", target.slug);
                        return;
                    }
                };
                // the slug already carries its date stamp — no extra prefix
                if let Err(error) = trigger_zip_download(&format!("{}.zip", target.slug), &bytes) {
                    log::warn!("export download of {} failed: {error:?}", target.slug);
                }
            }
            ExportForm::JsonToClipboard => {
                let files = match handle.read_all_files() {
                    Ok(files) => files,
                    Err(error) => {
                        log::warn!("export of {} failed: {error}", target.slug);
                        return;
                    }
                };
                match PackageEnvelope::encode(&target.slug, &files).to_json() {
                    Ok(json) => crate::clipboard::write_text(&json),
                    Err(error) => log::warn!("encoding {} failed: {error}", target.slug),
                }
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn export_package_as(_target: ExportTarget, _form: ExportForm) {}

/// Download a package as a zip (the gallery card's affordance).
pub(crate) fn export_package_to_download(card: &UiPackageCard) {
    export_package_as(ExportTarget::from(card), ExportForm::Zip);
}

/// Hand `bytes` to the browser as a named `.zip` download.
#[cfg(target_arch = "wasm32")]
pub(crate) fn trigger_zip_download(
    file_name: &str,
    bytes: &[u8],
) -> Result<(), wasm_bindgen::JsValue> {
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
