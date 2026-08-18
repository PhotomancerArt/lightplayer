use crate::providers::browser_worker::BrowserTickMode;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserWorkerOptions {
    pub fw_browser_module_path: String,
    pub fw_browser_wasm_path: String,
    /// Clock ownership mode for spawned workers (defaults to self-ticking).
    pub tick_mode: BrowserTickMode,
}

impl BrowserWorkerOptions {
    pub fn new(
        fw_browser_module_path: impl Into<String>,
        fw_browser_wasm_path: impl Into<String>,
    ) -> Self {
        Self {
            fw_browser_module_path: fw_browser_module_path.into(),
            fw_browser_wasm_path: fw_browser_wasm_path.into(),
            tick_mode: BrowserTickMode::SelfTicking,
        }
    }

    /// Set the worker clock ownership mode.
    pub fn with_tick_mode(mut self, tick_mode: BrowserTickMode) -> Self {
        self.tick_mode = tick_mode;
        self
    }

    pub fn worker_script_path(&self) -> String {
        wasm_bindgen::link_to!(module = "/src/providers/browser_worker/fw_browser_worker.js")
    }
}

impl Default for BrowserWorkerOptions {
    fn default() -> Self {
        Self::new("/pkg/fw_browser.js", "/pkg/fw_browser_bg.wasm")
    }
}

/// Resolve engine asset URLs from the pre-boot manifest fetch the shell
/// starts in `index.html` (`window.__lpEngineAssets`, a `Promise` of the
/// parsed `pkg/engine-manifest.json`, or `null` — see the P2 phase notes on
/// hashed engine sidecars). The manifest carries CONTENT-HASHED names
/// (`/pkg/fw_browser-<hash>.js`), which is what puts these multi-MB files on
/// `lp-cloud-server`'s immutable cache tier. A served Studio build no longer
/// carries the unhashed names at all — `Default::default()`'s constants
/// survive only as the native/test fallback and as what the standalone
/// fw-browser smoke page (`lp-fw/fw-browser/www`, its own static tree) still
/// serves.
///
/// Every failure mode along the way — no `window`, the global absent (an
/// `index.html` that predates this fetch, or a cached document), the
/// promise rejecting, the resolved value not being an object, a key missing
/// or not a string — falls back to [`BrowserWorkerOptions::default`] rather
/// than surfacing an error: a worker that boots from the unhashed name
/// still boots (revalidated, not immutable-cached), which beats refusing to
/// boot over a discovery hiccup.
#[cfg(target_arch = "wasm32")]
pub async fn resolved_engine_urls() -> BrowserWorkerOptions {
    try_resolve_engine_urls().await.unwrap_or_default()
}

#[cfg(target_arch = "wasm32")]
async fn try_resolve_engine_urls() -> Option<BrowserWorkerOptions> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window()?;
    let global = js_sys::Reflect::get(&window, &JsValue::from_str("__lpEngineAssets")).ok()?;
    let promise: js_sys::Promise = global.dyn_into().ok()?;
    let manifest = JsFuture::from(promise).await.ok()?;
    if manifest.is_null() || manifest.is_undefined() || !manifest.is_object() {
        return None;
    }
    let fw_browser_module_path =
        js_sys::Reflect::get(&manifest, &JsValue::from_str("fw_browser_js"))
            .ok()?
            .as_string()?;
    let fw_browser_wasm_path =
        js_sys::Reflect::get(&manifest, &JsValue::from_str("fw_browser_wasm"))
            .ok()?
            .as_string()?;
    Some(BrowserWorkerOptions::new(
        fw_browser_module_path,
        fw_browser_wasm_path,
    ))
}
