//! Error type for OPFS operations, carrying op + path context.

use std::fmt;

use wasm_bindgen::JsValue;

/// An OPFS operation failed.
#[derive(Debug, Clone)]
pub struct OpfsError {
    pub op: &'static str,
    pub path: String,
    pub message: String,
}

impl OpfsError {
    pub fn new(op: &'static str, path: impl Into<String>, js: JsValue) -> Self {
        let message = js
            .as_string()
            .or_else(|| {
                js_sys::Reflect::get(&js, &"message".into())
                    .ok()
                    .and_then(|m| m.as_string())
            })
            .unwrap_or_else(|| format!("{js:?}"));
        Self {
            op,
            path: path.into(),
            message,
        }
    }
}

impl fmt::Display for OpfsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "opfs {} failed for {}: {}",
            self.op, self.path, self.message
        )
    }
}

impl std::error::Error for OpfsError {}
