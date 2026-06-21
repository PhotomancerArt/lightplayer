use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserInputEnvelope {
    Boot {
        label: String,
        fw_browser_module_path: String,
        fw_browser_wasm_path: String,
    },
    ProtocolIn {
        frame: String,
    },
    Tick {
        delta_ms: Option<u32>,
    },
    Start,
    Stop,
    Drain,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserOutputEnvelope {
    Status {
        #[serde(default)]
        runtime_id: Option<u32>,
        status: String,
        message: Option<String>,
    },
    Log {
        runtime_id: u32,
        level: String,
        target: String,
        message: String,
    },
    ProtocolOut {
        frame: String,
    },
}
