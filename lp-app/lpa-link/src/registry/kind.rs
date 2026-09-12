use std::str::FromStr;

use crate::{LinkCapabilities, LinkOperation};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter, EnumString, IntoStaticStr};

/// Stable built-in provider class.
///
/// A kind is the identity of a provider implementation, not a configured
/// instance id. The current link model has at most one provider per kind in a
/// registry. String conversions use kebab-case keys such as
/// `browser-serial-esp32`, derived by `strum`/`serde` from the enum variants.
#[derive(
    Clone,
    Copy,
    Debug,
    Deserialize,
    Display,
    EnumIter,
    EnumString,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    Serialize,
    IntoStaticStr,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum LinkProviderKind {
    /// Test provider with in-memory fake endpoints and diagnostics.
    Fake,
    /// Host process provider that spawns local `fw-host` runtimes.
    HostProcess,
    /// Host serial provider for ESP32 hardware over OS serial ports.
    HostSerialEsp32,
    /// Browser worker provider backed by `fw-browser`.
    BrowserWorker,
    /// Browser Web Serial provider for ESP32 hardware and flashing.
    BrowserSerialEsp32,
    /// An ESP32-C6 emulated in this tab's own Worker, running the shipped
    /// firmware image over an emulated USB-Serial-JTAG link.
    EmulatorTab,
}

impl LinkProviderKind {
    /// Stable kebab-case key used in serialized state and app boundaries.
    pub fn key(self) -> &'static str {
        self.into()
    }

    /// Alias for `key` for call sites that want string-like access.
    pub fn as_str(&self) -> &'static str {
        self.key()
    }

    /// Parse a provider key, returning `None` for unknown keys.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::from_str(key).ok()
    }

    /// Technical label supplied by `lpa-link`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Fake => "Fake",
            Self::HostProcess => "Host process",
            Self::HostSerialEsp32 => "Host serial ESP32",
            Self::BrowserWorker => "Browser worker",
            Self::BrowserSerialEsp32 => "Browser serial ESP32",
            Self::EmulatorTab => "Emulated board in this tab",
        }
    }

    /// Transport label for UI surfaces that name how a DEVICE is reached.
    ///
    /// Every kind names itself: a runtime is reached over a channel like
    /// anything else, and a device backed by the browser worker is a **sim**
    /// — a device whose transport is a worker rather than a wire. `Fake` is
    /// the test double for serial hardware, so it wears the serial label and
    /// fixtures render like production. An emulated board is an **emu**: it
    /// runs the target's own firmware image rather than the desktop one, so
    /// calling it a sim would claim the wrong thing about what it is. Future
    /// device classes (websocket, network) name themselves here, which is
    /// why the answer stays optional.
    pub fn transport_label(self) -> Option<&'static str> {
        match self {
            Self::HostSerialEsp32 | Self::BrowserSerialEsp32 | Self::Fake => Some("USB"),
            Self::BrowserWorker => Some("sim"),
            Self::EmulatorTab => Some("emu"),
            Self::HostProcess => Some("host"),
        }
    }

    /// Baseline provider-class capabilities before endpoint/session specifics.
    pub fn capabilities(self) -> LinkCapabilities {
        match self {
            Self::Fake => LinkCapabilities::diagnostics_only(),
            Self::HostProcess | Self::BrowserWorker => LinkCapabilities::default()
                .with(LinkOperation::ReadLogs)
                .with(LinkOperation::ReadDiagnostics),
            // Native espflash-lib management (M5): reset, flash, and device
            // erase are all served in-process.
            Self::HostSerialEsp32 => LinkCapabilities::esp32_serial_base()
                .with_flash()
                .with_device_erase(),
            Self::BrowserSerialEsp32 => LinkCapabilities::esp32_serial_base().with_flash(),
            // Flash and erase are served in-process (the chip is a byte
            // array the page can address), the way the native serial
            // provider serves them — no ROM downloader in the middle.
            Self::EmulatorTab => LinkCapabilities::esp32_serial_base()
                .with_flash()
                .with_device_erase(),
        }
    }

    /// Static provider descriptor for this built-in kind.
    pub fn descriptor(self) -> crate::providers::LinkProviderDescriptor {
        crate::providers::LinkProviderDescriptor::new(self, self.label(), self.capabilities())
    }
}
