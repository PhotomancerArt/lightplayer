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
