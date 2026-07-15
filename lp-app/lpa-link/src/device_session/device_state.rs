//! The observable device state machine.
//!
//! Text form of the transitions the session drives:
//!
//! ```text
//! connect() ──▶ Booting ──hello(proto ok)──────────────▶ Ready { hello }
//!                 │
//!                 ├─boot lines match no-firmware sig──▶ BlankFlash
//!                 │                                     Bootloader
//!                 │                                     ForeignFirmware
//!                 ├─non-hello frame / wrong proto──────▶ Incompatible
//!                 ├─deadline, server marker seen───────▶ Incompatible (NoHello)
//!                 ├─deadline, no classification────────▶ Unresponsive
//!                 └─stream EOF / transport lost────────▶ Gone
//! Ready ──transport lost / close()──────────────────────▶ Gone
//! ```
//!
//! Reconnect/reflash flows (P3) are the only way OUT of the terminal
//! diagnosis states; this type only records where the session landed.

use lpc_wire::{ServerHello, WIRE_PROTO_VERSION};

use super::device_readiness::{BootDiagnosis, NO_FIRMWARE_DETECTED_PREFIX};

/// Where a hardware device session currently stands.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceState {
    /// ROM download mode observed. (Talking TO the bootloader is a later
    /// milestone; here it is a diagnosis.)
    Bootloader,
    /// Blank or erased flash observed (repeating invalid-header boot lines).
    BlankFlash,
    /// Recognized non-LightPlayer firmware that is safe to replace.
    ForeignFirmware,
    /// Port open, boot output flowing or awaited; not ready yet.
    Booting,
    /// The unsolicited wire hello arrived and its proto matches: the app
    /// protocol is available.
    Ready { hello: ServerHello },
    /// An `M!`-speaking peer that is not a compatible LightPlayer server.
    /// One affordance: reflash.
    Incompatible { reason: IncompatibleReason },
    /// The readiness deadline expired without any classification.
    Unresponsive { diagnosis: BootDiagnosis },
    /// Stream EOF / device vanished / session closed.
    Gone,
}

impl DeviceState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready { .. })
    }

    /// The hello payload when ready.
    pub fn hello(&self) -> Option<&ServerHello> {
        match self {
            Self::Ready { hello } => Some(hello),
            _ => None,
        }
    }

    /// Why the app protocol is unavailable in this state (`None` when
    /// [`Self::Ready`]). Used verbatim as the channel's gate error, so the
    /// no-firmware states keep the classifiable
    /// [`NO_FIRMWARE_DETECTED_PREFIX`].
    pub fn unavailable_message(&self) -> Option<String> {
        match self {
            Self::Ready { .. } => None,
            Self::Bootloader => Some(format!(
                "{NO_FIRMWARE_DETECTED_PREFIX}; ESP32 is waiting in ROM download mode"
            )),
            Self::BlankFlash => Some(format!(
                "{NO_FIRMWARE_DETECTED_PREFIX}; ESP32 boot output looks like blank or erased flash"
            )),
            Self::ForeignFirmware => Some(format!(
                "{NO_FIRMWARE_DETECTED_PREFIX}; ESP32 is running known replaceable non-LightPlayer firmware"
            )),
            Self::Booting => Some("device is still booting".to_string()),
            Self::Incompatible { reason } => Some(reason.message()),
            Self::Unresponsive { diagnosis } => Some(diagnosis.message()),
            Self::Gone => Some("device link is gone".to_string()),
        }
    }
}

/// Why an `M!`-speaking peer was rejected by the hello gate.
#[derive(Clone, Debug, PartialEq)]
pub enum IncompatibleReason {
    /// A decoded protocol frame that was not a hello arrived before any
    /// hello: a peer speaking some other (pre-hello) dialect.
    FrameBeforeHello,
    /// The LightPlayer server-start marker was observed but no hello arrived
    /// within the readiness deadline: pre-hello firmware (absence of a hello
    /// IS the mismatch signal — see the wire-hello ADR).
    NoHello,
    /// A hello arrived with a wire proto other than [`WIRE_PROTO_VERSION`].
    ProtoMismatch { hello: ServerHello },
}

impl IncompatibleReason {
    /// User-facing explanation. The single affordance is reflashing the
    /// firmware; nothing else can make this peer compatible.
    pub fn message(&self) -> String {
        match self {
            Self::FrameBeforeHello => {
                "device speaks the LightPlayer wire framing but did not identify itself with a \
                 hello; reflash the firmware to a compatible build"
                    .to_string()
            }
            Self::NoHello => {
                "device firmware started its server loop but predates the wire hello; reflash \
                 the firmware to a compatible build"
                    .to_string()
            }
            Self::ProtoMismatch { hello } => format!(
                "device firmware speaks wire protocol {} but this build speaks {}; reflash the \
                 firmware to a compatible build",
                hello.proto, WIRE_PROTO_VERSION
            ),
        }
    }
}
