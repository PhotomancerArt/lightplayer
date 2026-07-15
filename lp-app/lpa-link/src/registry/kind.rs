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
        }
    }

    /// Transport label for UI surfaces that name how a DEVICE is reached.
    ///
    /// `None` for runtime providers that are not devices at all (the browser
    /// worker and host process simulators never show as devices — D22).
    /// `Fake` is the test double for serial hardware, so it wears the serial
    /// label and fixtures render like production. Future device classes
    /// (websocket, network) name themselves here.
    pub fn transport_label(self) -> Option<&'static str> {
        match self {
            Self::HostSerialEsp32 | Self::BrowserSerialEsp32 | Self::Fake => Some("USB"),
            Self::HostProcess | Self::BrowserWorker => None,
        }
    }

    /// Baseline provider-class capabilities before endpoint/session specifics.
    pub fn capabilities(self) -> LinkCapabilities {
        match self {
            Self::Fake => LinkCapabilities::diagnostics_only(),
            Self::HostProcess | Self::BrowserWorker => LinkCapabilities::default()
                .with(LinkOperation::ReadLogs)
                .with(LinkOperation::ReadDiagnostics),
            // Logs + diagnostics only until the host provider grows a real
            // `manage()` implementation (M5 restores Reset with Flash/Erase).
            Self::HostSerialEsp32 => LinkCapabilities::diagnostics_and_logs(),
            Self::BrowserSerialEsp32 => LinkCapabilities::esp32_serial_base().with_flash(),
        }
    }

    /// Static provider descriptor for this built-in kind.
    pub fn descriptor(self) -> crate::providers::LinkProviderDescriptor {
        crate::providers::LinkProviderDescriptor::new(self, self.label(), self.capabilities())
    }
}
