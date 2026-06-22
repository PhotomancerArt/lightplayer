#![cfg_attr(
    not(all(feature = "browser-serial-esp32", target_arch = "wasm32")),
    allow(
        dead_code,
        reason = "browser serial readiness runs only in the wasm Web Serial adapter"
    )
)]

pub const NO_FIRMWARE_DETECTED_PREFIX: &str = "no LightPlayer firmware detected";

const RECENT_LINE_LIMIT: usize = 80;
const FAILURE_SNIPPET_LINE_LIMIT: usize = 6;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BrowserSerialReadinessClassifier {
    recent_lines: Vec<String>,
    invalid_blank_header_count: usize,
}

impl BrowserSerialReadinessClassifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe_line(&mut self, line: impl Into<String>) {
        let line = line.into();
        let normalized = line.to_ascii_lowercase();
        if normalized.contains("invalid header: 0xffffffff") {
            self.invalid_blank_header_count += 1;
        }
        self.recent_lines.push(line);
        if self.recent_lines.len() > RECENT_LINE_LIMIT {
            let remove_count = self.recent_lines.len() - RECENT_LINE_LIMIT;
            self.recent_lines.drain(0..remove_count);
        }
    }

    pub fn classify_timeout(&self) -> BrowserSerialReadinessFailure {
        if self.no_firmware_detected() {
            BrowserSerialReadinessFailure::NoFirmwareDetected {
                recent_lines: self.recent_lines.clone(),
            }
        } else {
            BrowserSerialReadinessFailure::ProtocolTimeout {
                recent_lines: self.recent_lines.clone(),
            }
        }
    }

    pub fn no_firmware_detected(&self) -> bool {
        self.invalid_blank_header_count > 0
    }

    pub fn recent_lines(&self) -> &[String] {
        &self.recent_lines
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserSerialReadinessFailure {
    NoFirmwareDetected { recent_lines: Vec<String> },
    ProtocolTimeout { recent_lines: Vec<String> },
}

impl BrowserSerialReadinessFailure {
    pub fn message(&self) -> String {
        match self {
            Self::NoFirmwareDetected { recent_lines } => {
                let mut message = format!(
                    "{NO_FIRMWARE_DETECTED_PREFIX}; ESP32 boot output looks like blank or erased flash"
                );
                append_recent_lines(&mut message, recent_lines);
                message
            }
            Self::ProtocolTimeout { recent_lines } => {
                let mut message =
                    "timed out waiting for browser serial server readiness".to_string();
                append_recent_lines(&mut message, recent_lines);
                message
            }
        }
    }
}

pub fn is_no_firmware_detected_message(message: &str) -> bool {
    message.contains(NO_FIRMWARE_DETECTED_PREFIX)
}

fn append_recent_lines(message: &mut String, recent_lines: &[String]) {
    let Some(summary) = recent_line_summary(recent_lines) else {
        return;
    };
    message.push_str("; recent serial output: ");
    message.push_str(&summary);
}

fn recent_line_summary(recent_lines: &[String]) -> Option<String> {
    if recent_lines.is_empty() {
        return None;
    }
    let start = recent_lines
        .len()
        .saturating_sub(FAILURE_SNIPPET_LINE_LIMIT);
    Some(recent_lines[start..].join(" | "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_blank_header_classifies_as_no_firmware() {
        let mut classifier = BrowserSerialReadinessClassifier::new();

        classifier.observe_line("ESP-ROM:esp32c6-20220919");
        classifier.observe_line("invalid header: 0xffffffff");

        assert_eq!(
            classifier.classify_timeout(),
            BrowserSerialReadinessFailure::NoFirmwareDetected {
                recent_lines: vec![
                    "ESP-ROM:esp32c6-20220919".to_string(),
                    "invalid header: 0xffffffff".to_string(),
                ],
            }
        );
    }

    #[test]
    fn unrelated_boot_output_classifies_as_protocol_timeout() {
        let mut classifier = BrowserSerialReadinessClassifier::new();

        classifier.observe_line("ESP-ROM:esp32c6-20220919");
        classifier.observe_line("[INIT] fw-esp32 starting...");

        assert!(matches!(
            classifier.classify_timeout(),
            BrowserSerialReadinessFailure::ProtocolTimeout { .. }
        ));
    }

    #[test]
    fn failure_message_includes_recent_serial_lines() {
        let failure = BrowserSerialReadinessFailure::ProtocolTimeout {
            recent_lines: vec![
                "line 1".to_string(),
                "line 2".to_string(),
                "line 3".to_string(),
                "line 4".to_string(),
                "line 5".to_string(),
                "line 6".to_string(),
                "line 7".to_string(),
            ],
        };

        let message = failure.message();

        assert!(message.contains("line 2 | line 3 | line 4 | line 5 | line 6 | line 7"));
        assert!(!message.contains("line 1 |"));
    }

    #[test]
    fn no_firmware_prefix_can_be_recovered_after_transport_wrapping() {
        assert!(is_no_firmware_detected_message(&format!(
            "Transport error: {NO_FIRMWARE_DETECTED_PREFIX}; recent serial output: invalid header"
        )));
    }
}
