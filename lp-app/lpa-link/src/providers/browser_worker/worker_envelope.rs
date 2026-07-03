use serde::{Deserialize, Serialize};

/// How the browser worker advances the firmware clock.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTickMode {
    /// The worker owns a timer and ticks with real measured deltas.
    ///
    /// This is the mode used by the Studio simulator so previews animate at
    /// roughly real time even when no protocol request is in flight.
    #[default]
    SelfTicking,
    /// Time advances only when the host sends an explicit `tick` envelope.
    ///
    /// Deterministic mode used by tests, stories, and emulator-style harnesses.
    Explicit,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserInputEnvelope {
    Boot {
        label: String,
        fw_browser_module_path: String,
        fw_browser_wasm_path: String,
        tick_mode: BrowserTickMode,
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
