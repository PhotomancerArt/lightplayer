#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserWorkerOptions {
    pub fw_browser_module_path: String,
    pub fw_browser_wasm_path: String,
}

impl BrowserWorkerOptions {
    pub fn new(
        fw_browser_module_path: impl Into<String>,
        fw_browser_wasm_path: impl Into<String>,
    ) -> Self {
        Self {
            fw_browser_module_path: fw_browser_module_path.into(),
            fw_browser_wasm_path: fw_browser_wasm_path.into(),
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn worker_script_path(&self) -> String {
        wasm_bindgen::link_to!(module = "/src/providers/browser_worker/fw_browser_worker.js")
    }
}

impl Default for BrowserWorkerOptions {
    fn default() -> Self {
        Self::new("./pkg/fw_browser.js", "./pkg/fw_browser_bg.wasm")
    }
}
