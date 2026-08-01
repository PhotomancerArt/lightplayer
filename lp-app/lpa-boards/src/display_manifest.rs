//! Serde types for `boards/<vendor>/<product>.display.json` sidecars.
//!
//! A display sidecar carries everything the boards catalog, provisioning
//! picker, and diagram renderer need that the runtime manifest must not:
//! commerce/identity fields and the drawing block. Pin facts here are
//! presentation-level; the runtime manifest stays the authority on claimable
//! resources, and `tests/manifest_drift.rs` keeps the two consistent.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::usb_bridge::UsbBridge;

/// One board's display sidecar (`<vendor>/<product>.display.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct BoardDisplayFile {
    /// Must equal the sibling runtime manifest id and the `vendor/product`
    /// file path stem.
    pub board_id: String,
    pub display_name: String,
    pub manufacturer: String,
    /// Human SoC name, e.g. "ESP32-C6".
    pub soc: String,
    /// Filter family for the provisioning picker: "esp32", "esp32s3",
    /// "esp32c6". Kept a plain string so display data never gates on firmware
    /// target enums.
    pub family: String,
    pub flash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub psram: Option<String>,
    /// Approximate street price, USD, for catalog sorting.
    pub price_usd: f64,
    pub tier: SupportTier,
    /// Catalog capability chips ("Wi-Fi 6", "level shifter", ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<String>,
    pub blurb: String,
    /// Honest support caveat shown with the tier, e.g. "runs when classic
    /// ESP32 firmware lands".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub purchase_urls: Vec<PurchaseUrl>,
    /// The USB-UART bridge the board's USB connector goes through — drives
    /// per-OS driver warnings (plan decision D5). Set only when verified;
    /// omitted = unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usb_bridge: Option<UsbBridge>,
    /// Freeform board notes shown on the detail view, optionally OS-tagged
    /// (an untagged note shows everywhere; a tagged one only on that OS).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<BoardNote>,
    pub hw: BoardDrawing,
}

/// One detail-view note. Deliberately minimal: text plus optional OS (and
/// freeform OS-version qualifier) tags — the whole "note system".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct BoardNote {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<NoteOs>,
    /// Freeform version qualifier shown with the OS tag ("Sequoia+").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_version: Option<String>,
}

/// The OS a note applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum NoteOs {
    #[serde(rename = "macos")]
    MacOs,
    Windows,
    Linux,
}

impl NoteOs {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::MacOs => "macOS",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
        }
    }

    /// Whether a note tagged with this OS applies on `host`.
    pub fn matches(self, host: crate::usb_bridge::HostOs) -> bool {
        use crate::usb_bridge::HostOs;
        matches!(
            (self, host),
            (Self::MacOs, HostOs::MacOs)
                | (Self::Windows, HostOs::Windows)
                | (Self::Linux, HostOs::Linux)
        )
    }
}

/// Support tier shown on the catalog. Definitions live in
/// `lp-core/lpc-hardware/boards/README.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum SupportTier {
    /// First-class: tested every release.
    Gold,
    /// Supported: tested occasionally.
    Silver,
    /// Community-verified: should work.
    Bronze,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct PurchaseUrl {
    pub label: String,
    pub href: String,
}

/// The drawing block the row-engine diagram renderer consumes.
///
/// Geometry is in board-local units; the renderer derives every other size
/// from the pin pitch (`u`). See `docs/design/board-diagrams.md`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct BoardDrawing {
    /// Board width in drawing units. Height derives from pin counts.
    pub width: f32,
    pub module: DrawnModule,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub usb: Vec<DrawnUsb>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buttons: Vec<DrawnButton>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rgb: Option<DrawnRgb>,
    /// Top-edge screw terminals (band rows + leader lines).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminals: Vec<DrawnTerminal>,
    pub left: Vec<DrawnPin>,
    pub right: Vec<DrawnPin>,
}

/// The SoC module / can outline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct DrawnModule {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: String,
    #[serde(default)]
    pub antenna: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct DrawnUsb {
    pub x: f32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct DrawnButton {
    pub x: f32,
    /// Negative y = offset from the board's bottom edge.
    pub y: f32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct DrawnRgb {
    pub x: f32,
    pub y: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpio: Option<u8>,
}

/// A top-edge screw terminal. Terminal rows keep a leading name cell because
/// there is no inside-the-board space for their labels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct DrawnTerminal {
    pub label: String,
    pub role: PinRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpio: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caps: Vec<PinCap>,
}

/// One header pin on a left/right rail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct DrawnPin {
    /// Silkscreen label ("4", "3V3", "D10", "RST").
    pub label: String,
    pub role: PinRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpio: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub caps: Vec<PinCap>,
}

/// Pin role: drives pad/label color and output eligibility.
///
/// Output-eligible for LED discovery: `io` and `strap` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum PinRole {
    Pwr5,
    Pwr3,
    Gnd,
    Io,
    /// Input-only (classic ESP32 34-39).
    IoIn,
    /// Boot-strap pin: usable, with care.
    Strap,
    /// USB D+/D- — driving it drops the link.
    Usb,
    /// EN / RST.
    Ctl,
    Nc,
    /// Connected but reserved (in-package flash/PSRAM pins on boards that
    /// expose them). Never claimable, never discovery-eligible.
    Rsvd,
}

impl PinRole {
    /// Whether LED discovery may drive this pin.
    pub fn output_eligible(self) -> bool {
        matches!(self, Self::Io | Self::Strap)
    }
}

/// A capability cell in the pin's row ("ADC1_3", "Touch4", "U0_TXD").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct PinCap {
    pub text: String,
    pub kind: CapKind,
}

/// Cell type: drives the capability cell's color family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum CapKind {
    Adc,
    Dac,
    Touch,
    Spi,
    I2c,
    Uart,
    Usb,
    Strap,
    Pwr,
    /// Caution: input-only, XTAL, PSRAM-reserved, JTAG, onboard LED.
    Warn,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardDisplayError {
    Parse { message: String },
    Invalid { message: String },
}

impl fmt::Display for BoardDisplayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { message } => write!(f, "display manifest parse error: {message}"),
            Self::Invalid { message } => write!(f, "invalid display manifest: {message}"),
        }
    }
}

impl std::error::Error for BoardDisplayError {}

impl BoardDisplayFile {
    pub fn read_json(json_text: &str) -> Result<Self, BoardDisplayError> {
        let file: Self =
            serde_json::from_str(json_text).map_err(|error| BoardDisplayError::Parse {
                message: error.to_string(),
            })?;
        file.validate()?;
        Ok(file)
    }

    /// Structural checks that don't need the runtime manifest. Cross-file
    /// consistency (board_id vs path, runtime agreement) lives in the drift
    /// tests, which see the filesystem.
    pub fn validate(&self) -> Result<(), BoardDisplayError> {
        let invalid = |message: String| Err(BoardDisplayError::Invalid { message });
        if self.board_id.split('/').count() != 2
            || self.board_id.split('/').any(|part| part.trim().is_empty())
        {
            return invalid(format!(
                "board_id must be vendor/product, got {:?}",
                self.board_id
            ));
        }
        if self.display_name.trim().is_empty() {
            return invalid("display_name must not be empty".into());
        }
        if !self.price_usd.is_finite() || self.price_usd < 0.0 {
            return invalid(format!(
                "price_usd must be non-negative: {}",
                self.price_usd
            ));
        }
        for url in &self.purchase_urls {
            if !(url.href.starts_with("https://") || url.href.starts_with("http://")) {
                return invalid(format!("purchase url must be http(s): {}", url.href));
            }
        }
        let mut seen = Vec::new();
        for pin in self.pins() {
            if pin.label.trim().is_empty() {
                return invalid("pin label must not be empty".into());
            }
            if let Some(gpio) = pin.gpio {
                if seen.contains(&gpio) {
                    return invalid(format!("duplicate gpio {gpio} in pin tables"));
                }
                seen.push(gpio);
            }
            // Numeric silkscreen labels ARE the gpio number; a mismatch (or a
            // missing gpio) is a physical-damage class of authoring mistake.
            // Named pins (D4, LED1) may omit gpio when unverified.
            if let Ok(numeric) = pin.label.parse::<u8>() {
                if pin.gpio != Some(numeric) {
                    return invalid(format!(
                        "pin labeled {:?} must carry gpio {numeric}, got {:?}",
                        pin.label, pin.gpio
                    ));
                }
            }
        }
        for terminal in &self.hw.terminals {
            if let Some(gpio) = terminal.gpio {
                if seen.contains(&gpio) {
                    return invalid(format!(
                        "duplicate gpio {gpio} (terminal {})",
                        terminal.label
                    ));
                }
                seen.push(gpio);
            }
        }
        Ok(())
    }

    /// All rail pins, left then right (terminals excluded).
    pub fn pins(&self) -> impl Iterator<Item = &DrawnPin> {
        self.hw.left.iter().chain(self.hw.right.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal() -> BoardDisplayFile {
        BoardDisplayFile {
            board_id: "vendor/product".into(),
            display_name: "Board".into(),
            manufacturer: "Vendor".into(),
            soc: "ESP32-C6".into(),
            family: "esp32c6".into(),
            flash: "8 MB".into(),
            psram: None,
            price_usd: 9.0,
            tier: SupportTier::Bronze,
            capabilities: vec![],
            blurb: "A board.".into(),
            support_note: None,
            purchase_urls: vec![],
            usb_bridge: None,
            notes: vec![],
            hw: BoardDrawing {
                width: 100.0,
                module: DrawnModule {
                    x: 10.0,
                    y: 8.0,
                    w: 80.0,
                    h: 60.0,
                    label: "SOC".into(),
                    antenna: true,
                },
                usb: vec![],
                buttons: vec![],
                rgb: None,
                terminals: vec![],
                left: vec![DrawnPin {
                    label: "4".into(),
                    role: PinRole::Io,
                    gpio: Some(4),
                    caps: vec![],
                }],
                right: vec![],
            },
        }
    }

    #[test]
    fn accepts_minimal() {
        minimal().validate().unwrap();
    }

    #[test]
    fn rejects_duplicate_gpio() {
        let mut file = minimal();
        file.hw.right = file.hw.left.clone();
        assert!(matches!(
            file.validate(),
            Err(BoardDisplayError::Invalid { .. })
        ));
    }

    #[test]
    fn rejects_bad_board_id() {
        let mut file = minimal();
        file.board_id = "no-slash".into();
        assert!(file.validate().is_err());
    }

    #[test]
    fn output_eligibility_is_io_and_strap_only() {
        assert!(PinRole::Io.output_eligible());
        assert!(PinRole::Strap.output_eligible());
        for role in [
            PinRole::Pwr5,
            PinRole::Pwr3,
            PinRole::Gnd,
            PinRole::IoIn,
            PinRole::Usb,
            PinRole::Ctl,
            PinRole::Nc,
            PinRole::Rsvd,
        ] {
            assert!(!role.output_eligible(), "{role:?} must not be eligible");
        }
    }

    #[test]
    fn round_trips_json() {
        let file = minimal();
        let json = serde_json::to_string_pretty(&file).unwrap();
        let back = BoardDisplayFile::read_json(&json).unwrap();
        assert_eq!(file, back);
    }
}
