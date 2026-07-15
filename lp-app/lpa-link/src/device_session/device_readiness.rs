//! Hello gate (readiness) + boot-line classifier (diagnosis).
//!
//! **Readiness is granted by exactly one thing**: the unsolicited wire
//! [`ServerHello`] whose `proto` matches [`WIRE_PROTO_VERSION`]
//! ([`gate_first_frame`]). Everything else in this file is DIAGNOSIS: the
//! [`BootLineClassifier`] watches non-protocol serial lines to explain why a
//! device is not ready (blank flash, ROM bootloader, known foreign firmware,
//! silence) and never grants readiness itself.
//!
//! This is the ONLY boot-line classifier in the app layer. It started life
//! in `lpa-studio-core` as a readiness-granting string grep; the device
//! session work moved it here and demoted it to diagnosis, and the studio
//! copy (and the client-io adapters that consumed it) were deleted. See
//! `docs/adr/2026-07-15-device-session-model.md`.

use lpc_wire::{ServerHello, ServerMsgBody, WIRE_PROTO_VERSION, WireServerMessage};

use super::device_state::IncompatibleReason;

/// Stable prefix for "this device has no LightPlayer firmware" messages, so
/// upper layers can classify the error after transport wrapping.
pub const NO_FIRMWARE_DETECTED_PREFIX: &str = "no LightPlayer firmware detected";

const RECENT_LINE_LIMIT: usize = 80;
const FAILURE_SNIPPET_LINE_LIMIT: usize = 6;
const SAFE_TO_REPLACE_FIRMWARE_BOOT_STRINGS: &[&str] = &["hello from seeed studio xiao esp32-c6"];

/// Outcome of gating the first protocol frame seen while booting.
#[derive(Debug)]
pub(crate) enum HelloGate {
    /// The frame is a hello with the wire proto this build speaks.
    Ready(ServerHello),
    /// The peer speaks `M!` frames but is not a compatible LightPlayer
    /// server: wrong proto, or a non-hello frame before any hello.
    Incompatible(IncompatibleReason),
}

/// Hello-first readiness: only a proto-matching [`ServerHello`] grants
/// readiness; any other decoded frame before a hello means an incompatible
/// (pre-hello or foreign-proto) peer.
pub(crate) fn gate_first_frame(frame: WireServerMessage) -> HelloGate {
    match frame.msg {
        ServerMsgBody::Hello(hello) if hello.proto == WIRE_PROTO_VERSION => HelloGate::Ready(hello),
        ServerMsgBody::Hello(hello) => {
            HelloGate::Incompatible(IncompatibleReason::ProtoMismatch { hello })
        }
        _ => HelloGate::Incompatible(IncompatibleReason::FrameBeforeHello),
    }
}

/// Streaming classifier over the device's non-protocol serial lines.
///
/// Diagnosis-only: it recognizes the ESP32 boot signatures of "no firmware"
/// states and the LightPlayer server-start marker, and keeps a bounded tail
/// of recent lines for error messages and snapshots.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BootLineClassifier {
    recent_lines: Vec<String>,
    invalid_blank_header_count: usize,
    rom_download_mode_count: usize,
    safe_to_replace_firmware_count: usize,
    server_started: bool,
}

impl BootLineClassifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn observe_line(&mut self, line: impl Into<String>) {
        let line = line.into();
        let normalized = line.to_ascii_lowercase();
        if normalized.contains("invalid header: 0xffffffff") {
            self.invalid_blank_header_count += 1;
        }
        if normalized.contains("waiting for download") || normalized.contains("(download(") {
            self.rom_download_mode_count += 1;
        }
        if SAFE_TO_REPLACE_FIRMWARE_BOOT_STRINGS
            .iter()
            .any(|known_boot_string| normalized.contains(known_boot_string))
        {
            self.safe_to_replace_firmware_count += 1;
        }
        if normalized.contains("fw-esp32 initialized, starting server loop") {
            self.server_started = true;
        }
        self.recent_lines.push(line);
        if self.recent_lines.len() > RECENT_LINE_LIMIT {
            let remove_count = self.recent_lines.len() - RECENT_LINE_LIMIT;
            self.recent_lines.drain(0..remove_count);
        }
    }

    /// Explain a readiness deadline expiry from what was (not) observed.
    pub fn classify_timeout(&self) -> BootDiagnosis {
        if self.no_firmware_detected() {
            BootDiagnosis::NoFirmwareDetected {
                recent_lines: self.recent_lines.clone(),
                reason: self.no_firmware_reason(),
            }
        } else if self.recent_lines.is_empty() {
            BootDiagnosis::NoSerialOutput
        } else {
            BootDiagnosis::NoHello {
                recent_lines: self.recent_lines.clone(),
            }
        }
    }

    pub fn no_firmware_detected(&self) -> bool {
        self.invalid_blank_header_count > 0
            || self.rom_download_mode_count > 0
            || self.safe_to_replace_firmware_count > 0
    }

    /// Whether the LightPlayer server-start boot marker was observed.
    ///
    /// Diagnosis-only: a started server that never hellos is PRE-HELLO
    /// firmware ([`IncompatibleReason::NoHello`]), not readiness.
    pub fn server_started(&self) -> bool {
        self.server_started
    }

    pub fn recent_lines(&self) -> &[String] {
        &self.recent_lines
    }

    /// The most specific no-firmware reason observed so far.
    pub fn no_firmware_reason(&self) -> NoFirmwareReason {
        if self.rom_download_mode_count > 0 {
            NoFirmwareReason::RomDownloadMode
        } else if self.safe_to_replace_firmware_count > 0 {
            NoFirmwareReason::SafeToReplaceFirmware
        } else {
            NoFirmwareReason::BlankOrErasedFlash
        }
    }
}

/// Why the device did not become ready, as diagnosed from boot lines.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BootDiagnosis {
    /// Nothing at all arrived on the wire.
    NoSerialOutput,
    /// Boot output matches a known no-firmware signature.
    NoFirmwareDetected {
        recent_lines: Vec<String>,
        reason: NoFirmwareReason,
    },
    /// Output flowed but no wire hello arrived.
    NoHello { recent_lines: Vec<String> },
}

/// Which no-firmware signature the boot output matched.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoFirmwareReason {
    BlankOrErasedFlash,
    RomDownloadMode,
    SafeToReplaceFirmware,
}

impl BootDiagnosis {
    pub fn message(&self) -> String {
        match self {
            Self::NoSerialOutput => "timed out waiting for device readiness; no serial output was received from the device".to_string(),
            Self::NoFirmwareDetected {
                recent_lines,
                reason,
            } => {
                let mut message = match reason {
                    NoFirmwareReason::BlankOrErasedFlash => format!(
                        "{NO_FIRMWARE_DETECTED_PREFIX}; ESP32 boot output looks like blank or erased flash"
                    ),
                    NoFirmwareReason::RomDownloadMode => format!(
                        "{NO_FIRMWARE_DETECTED_PREFIX}; ESP32 is waiting in ROM download mode"
                    ),
                    NoFirmwareReason::SafeToReplaceFirmware => format!(
                        "{NO_FIRMWARE_DETECTED_PREFIX}; ESP32 is running known replaceable non-LightPlayer firmware"
                    ),
                };
                append_recent_lines(&mut message, recent_lines);
                message
            }
            Self::NoHello { recent_lines } => {
                let mut message =
                    "timed out waiting for the device hello".to_string();
                append_recent_lines(&mut message, recent_lines);
                message
            }
        }
    }
}

/// Recover the no-firmware classification from a message that may have been
/// wrapped by transport error formatting.
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
    use lpc_wire::{FwProvenance, ServerMessage};

    use super::*;

    #[test]
    fn matching_proto_hello_passes_the_gate() {
        let hello = test_hello(WIRE_PROTO_VERSION);
        let frame = ServerMessage::new(0, ServerMsgBody::Hello(hello.clone()));

        assert!(matches!(
            gate_first_frame(frame),
            HelloGate::Ready(gated) if gated == hello
        ));
    }

    #[test]
    fn wrong_proto_hello_is_incompatible() {
        let hello = test_hello(WIRE_PROTO_VERSION + 1);
        let frame = ServerMessage::new(0, ServerMsgBody::Hello(hello.clone()));

        assert!(matches!(
            gate_first_frame(frame),
            HelloGate::Incompatible(IncompatibleReason::ProtoMismatch { hello: gated })
                if gated == hello
        ));
    }

    #[test]
    fn non_hello_frame_before_hello_is_incompatible() {
        let frame = ServerMessage::new(3, ServerMsgBody::UnloadProject);

        assert!(matches!(
            gate_first_frame(frame),
            HelloGate::Incompatible(IncompatibleReason::FrameBeforeHello)
        ));
    }

    #[test]
    fn invalid_blank_header_classifies_as_no_firmware() {
        let mut classifier = BootLineClassifier::new();

        classifier.observe_line("ESP-ROM:esp32c6-20220919");
        classifier.observe_line("invalid header: 0xffffffff");

        assert_eq!(
            classifier.classify_timeout(),
            BootDiagnosis::NoFirmwareDetected {
                recent_lines: vec![
                    "ESP-ROM:esp32c6-20220919".to_string(),
                    "invalid header: 0xffffffff".to_string(),
                ],
                reason: NoFirmwareReason::BlankOrErasedFlash,
            }
        );
    }

    #[test]
    fn rom_download_mode_classifies_as_no_firmware() {
        let mut classifier = BootLineClassifier::new();

        classifier.observe_line("boot:0x16 (DOWNLOAD(USB/UART0/SDIO_REI_FEO))");
        classifier.observe_line("waiting for download");

        assert_eq!(
            classifier.classify_timeout(),
            BootDiagnosis::NoFirmwareDetected {
                recent_lines: vec![
                    "boot:0x16 (DOWNLOAD(USB/UART0/SDIO_REI_FEO))".to_string(),
                    "waiting for download".to_string(),
                ],
                reason: NoFirmwareReason::RomDownloadMode,
            }
        );
        assert!(
            classifier
                .classify_timeout()
                .message()
                .contains("ESP32 is waiting in ROM download mode")
        );
    }

    #[test]
    fn known_replaceable_firmware_classifies_as_no_firmware() {
        let mut classifier = BootLineClassifier::new();

        classifier.observe_line("Hello from Seeed Studio XIAO ESP32-C6");

        assert_eq!(
            classifier.classify_timeout(),
            BootDiagnosis::NoFirmwareDetected {
                recent_lines: vec!["Hello from Seeed Studio XIAO ESP32-C6".to_string()],
                reason: NoFirmwareReason::SafeToReplaceFirmware,
            }
        );
        assert!(
            classifier
                .classify_timeout()
                .message()
                .contains("known replaceable non-LightPlayer firmware")
        );
    }

    #[test]
    fn unrelated_boot_output_classifies_as_no_hello() {
        let mut classifier = BootLineClassifier::new();

        classifier.observe_line("ESP-ROM:esp32c6-20220919");
        classifier.observe_line("[INIT] fw-esp32 starting...");

        assert!(matches!(
            classifier.classify_timeout(),
            BootDiagnosis::NoHello { .. }
        ));
    }

    #[test]
    fn server_loop_start_marks_server_started_but_grants_nothing() {
        let mut classifier = BootLineClassifier::new();

        classifier.observe_line("[INIT] fw-esp32 initialized, starting server loop...");

        assert!(classifier.server_started());
        assert!(!classifier.no_firmware_detected());
        // Diagnosis-only: the marker alone still classifies as no-hello.
        assert!(matches!(
            classifier.classify_timeout(),
            BootDiagnosis::NoHello { .. }
        ));
    }

    #[test]
    fn server_loop_start_with_version_suffix_still_marks_server_started() {
        // The M2 boot line carries proto/commit/dirty; the marker substring
        // must keep matching (it now feeds the pre-hello-firmware diagnosis).
        let mut classifier = BootLineClassifier::new();

        classifier.observe_line(
            "[INIT] fw-esp32 initialized, starting server loop... proto=1 commit=abc123456789 dirty=false",
        );

        assert!(classifier.server_started());
        assert!(!classifier.no_firmware_detected());
    }

    #[test]
    fn no_output_classifies_as_no_serial_output() {
        let classifier = BootLineClassifier::new();

        assert_eq!(classifier.classify_timeout(), BootDiagnosis::NoSerialOutput);
        assert!(
            classifier
                .classify_timeout()
                .message()
                .contains("no serial output")
        );
    }

    #[test]
    fn failure_message_includes_recent_serial_lines() {
        let diagnosis = BootDiagnosis::NoHello {
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

        let message = diagnosis.message();

        assert!(message.contains("line 2 | line 3 | line 4 | line 5 | line 6 | line 7"));
        assert!(!message.contains("line 1 |"));
    }

    #[test]
    fn no_firmware_prefix_can_be_recovered_after_transport_wrapping() {
        assert!(is_no_firmware_detected_message(&format!(
            "Transport error: {NO_FIRMWARE_DETECTED_PREFIX}; recent serial output: invalid header"
        )));
    }

    fn test_hello(proto: u32) -> ServerHello {
        ServerHello {
            proto,
            fw: FwProvenance {
                package: "fw-esp32".to_string(),
                commit: "test".to_string(),
                dirty: false,
                profile: "release-esp32".to_string(),
            },
            device_uid: None,
        }
    }
}
