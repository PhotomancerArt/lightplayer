//! Worker-console sink for `log::` records.
//!
//! Without a global logger every `log::` line produced inside the worker —
//! including shader-compile failures from the engine — is silently dropped.
//! This sink mirrors records to the worker's JS console, routing severity to
//! the matching `console.*` method.
//!
//! The logger itself is permissive: the process-global `log::max_level()` —
//! seeded to `Info` here and mutable at runtime via the wire `SetLogLevel`
//! command — is the single effective gate (same convention as the ESP32 and
//! emulator loggers). An internal cap would silently mask a raised global
//! level.

use std::sync::Once;

use log::{Level, LevelFilter, Log, Metadata, Record};
use wasm_bindgen::prelude::wasm_bindgen;

/// Default for the process-global `log::max_level()` gate applied at install.
const LOG_LEVEL: LevelFilter = LevelFilter::Info;

/// Install the console logger. Idempotent; safe to call from every export
/// entry point.
pub fn install() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        match log::set_logger(&ConsoleLogger) {
            Ok(()) => log::set_max_level(LOG_LEVEL),
            // Another logger got there first (only possible if the embedding
            // module installed one); keep it rather than panic.
            Err(_) => console_warn(
                "fw-browser console logger not installed: a global logger is already set",
            ),
        }
    });
}

/// Logger that mirrors records to the worker's JS console.
struct ConsoleLogger;

impl Log for ConsoleLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // Always enabled: `log::max_level()` is the runtime gate (enforced by
        // the `log::` macros before records reach the sink).
        true
    }

    fn log(&self, record: &Record) {
        let module_path = record.module_path().unwrap_or("unknown");
        // Severity is conveyed by the console method, not the text.
        let message = format!("[{}] {}", module_path, record.args());
        match record.level() {
            Level::Trace | Level::Debug => console_debug(&message),
            Level::Info => console_info(&message),
            Level::Warn => console_warn(&message),
            Level::Error => console_error(&message),
        }
    }

    fn flush(&self) {
        // No-op: console writes are synchronous.
    }
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = debug)]
    fn console_debug(message: &str);

    #[wasm_bindgen(js_namespace = console, js_name = info)]
    fn console_info(message: &str);

    #[wasm_bindgen(js_namespace = console, js_name = warn)]
    fn console_warn(message: &str);

    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}
