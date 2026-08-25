use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;
use lp_collection::VecSet;

use serde::{Deserialize, Serialize};

use crate::{
    HardwareTarget, HwAddress, HwCapability, HwError, HwGateLevel, HwManifest, HwPowerGate,
    HwResource, HwSoftLimits,
};

/// Serializable board manifest file (authored as JSON).
///
/// This is the serializable form checked into the repo for board profiles. Use
/// [`HardwareManifestFile::to_manifest`] to validate and convert it into the
/// runtime [`HwManifest`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct HardwareManifestFile {
    pub id: String,
    pub target: HardwareTarget,
    pub vendor: String,
    pub product: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Measured soft-limit records (see [`HwSoftLimits`]): envelopes this
    /// board×firmware has run clean at. Optional and additive — older
    /// firmware parsing a manifest that carries them simply ignores the
    /// field, which is what keeps this change format-compatible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soft_limits: Option<HwSoftLimits>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub board_label: Vec<HardwareBoardLabelFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpio: Vec<HardwareResourceFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource: Vec<HardwareResourceFile>,
    /// Board-level power-gate descriptors (see [`HwPowerGate`]). Additive and
    /// optional, like `soft_limits`: older firmware parsing a manifest that
    /// carries one simply never sees it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub power_gate: Vec<HardwarePowerGateFile>,
}

impl HardwareManifestFile {
    pub fn new(
        id: impl Into<String>,
        target: HardwareTarget,
        vendor: impl Into<String>,
        product: impl Into<String>,
    ) -> Self {
        let product = product.into();
        Self {
            id: id.into(),
            target,
            vendor: vendor.into(),
            product: product.clone(),
            description: None,
            url: None,
            soft_limits: None,
            board_label: Vec::new(),
            gpio: Vec::new(),
            resource: Vec::new(),
            power_gate: Vec::new(),
        }
    }

    pub fn read_json(json_text: &str) -> Result<Self, HardwareManifestFileError> {
        serde_json::from_str(json_text).map_err(|error| HardwareManifestFileError::Parse {
            message: error.to_string(),
        })
    }

    pub fn write_json(&self) -> Result<String, HardwareManifestFileError> {
        serde_json::to_string_pretty(self).map_err(|error| HardwareManifestFileError::Serialize {
            message: error.to_string(),
        })
    }

    pub fn validate(&self) -> Result<(), HardwareManifestFileError> {
        if self.id.trim().is_empty() {
            return Err(HardwareManifestFileError::Invalid {
                message: "id must not be empty".into(),
            });
        }
        if self.vendor.trim().is_empty() {
            return Err(HardwareManifestFileError::Invalid {
                message: "vendor must not be empty".into(),
            });
        }
        if self.product.trim().is_empty() {
            return Err(HardwareManifestFileError::Invalid {
                message: "product must not be empty".into(),
            });
        }
        if let Some(url) = &self.url {
            if !(url.starts_with("https://") || url.starts_with("http://")) {
                return Err(HardwareManifestFileError::Invalid {
                    message: "url must start with http:// or https://".into(),
                });
            }
        }

        let mut seen = VecSet::new();
        for label in &self.board_label {
            if label.label.trim().is_empty() {
                return Err(HardwareManifestFileError::Invalid {
                    message: "board_label label must not be empty".into(),
                });
            }
            if !seen.insert(label.label.trim().to_string()) {
                return Err(HardwareManifestFileError::Invalid {
                    message: alloc::format!("duplicate board label: {}", label.label),
                });
            }
        }

        let mut seen = VecSet::new();
        for resource in self.gpio.iter().chain(self.resource.iter()) {
            let address = HwAddress::new(resource.address.clone())?;
            if !seen.insert(address.clone()) {
                return Err(HardwareManifestFileError::Invalid {
                    message: alloc::format!("duplicate resource address: {address}"),
                });
            }
        }

        for gate in &self.power_gate {
            HwAddress::new(gate.gpio.clone())?;
            for feed in &gate.feeds {
                HwAddress::new(feed.clone())?;
            }
        }
        Ok(())
    }

    pub fn to_manifest(&self) -> Result<HwManifest, HardwareManifestFileError> {
        self.validate()?;
        let resources = self.resources()?;
        let power_gates = self.power_gates()?;
        let mut manifest = HwManifest::new(self.id.clone(), self.product.clone(), resources)
            .with_target(self.target)
            .with_vendor(self.vendor.clone())
            .with_product(self.product.clone())
            .with_power_gates(power_gates);
        if let Some(description) = &self.description {
            manifest = manifest.with_description(description.clone());
        }
        if let Some(url) = &self.url {
            manifest = manifest.with_url(url.clone());
        }
        if let Some(soft_limits) = &self.soft_limits {
            manifest = manifest.with_soft_limits(soft_limits.clone());
        }
        Ok(manifest)
    }

    fn resources(&self) -> Result<Vec<HwResource>, HardwareManifestFileError> {
        self.gpio
            .iter()
            .chain(self.resource.iter())
            .map(HardwareResourceFile::to_resource)
            .collect()
    }

    fn power_gates(&self) -> Result<Vec<HwPowerGate>, HardwareManifestFileError> {
        self.power_gate
            .iter()
            .map(HardwarePowerGateFile::to_power_gate)
            .collect()
    }
}

/// Board-silkscreen label discovered or recorded during board mapping.
///
/// Labels are metadata for humans and tooling. They do not create claimable
/// resources by themselves; resources come from [`HardwareResourceFile`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct HardwareBoardLabelFile {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<HardwareBoardLabelStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl HardwareBoardLabelFile {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            gpio: None,
            status: None,
            note: None,
        }
    }
}

/// Verification status for a board label in a manifest file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "kebab-case")]
pub enum HardwareBoardLabelStatus {
    Unassigned,
    Assigned,
    Verified,
    NotFound,
    Skipped,
}

/// Serializable resource entry in a board manifest file.
///
/// GPIO resources are often grouped under `gpio` in TOML for readability, while
/// non-GPIO resources live under `resource`; both convert to [`HwResource`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct HardwareResourceFile {
    pub address: String,
    pub display_label: String,
    pub capabilities: Vec<HwCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserved_reason: Option<String>,
}

impl HardwareResourceFile {
    pub fn new(
        address: impl Into<String>,
        display_label: impl Into<String>,
        capabilities: impl Into<Vec<HwCapability>>,
    ) -> Self {
        Self {
            address: address.into(),
            display_label: display_label.into(),
            capabilities: capabilities.into(),
            aliases: Vec::new(),
            location: None,
            reserved_reason: None,
        }
    }

    fn to_resource(&self) -> Result<HwResource, HardwareManifestFileError> {
        if self.display_label.trim().is_empty() {
            return Err(HardwareManifestFileError::Invalid {
                message: alloc::format!("{} display_label must not be empty", self.address),
            });
        }
        if self.capabilities.is_empty() {
            return Err(HardwareManifestFileError::Invalid {
                message: alloc::format!("{} must have at least one capability", self.address),
            });
        }
        let mut resource = HwResource::new(
            HwAddress::new(self.address.clone())?,
            self.capabilities.clone(),
            self.display_label.clone(),
        )
        .with_aliases(self.aliases.clone());
        if let Some(location) = &self.location {
            resource = resource.with_location(location.clone());
        }
        if let Some(reason) = &self.reserved_reason {
            resource = resource.reserved(reason.clone());
        }
        Ok(resource)
    }
}

/// Serializable power-gate entry in a board manifest file (see
/// [`HwPowerGate`]). `gpio`/`feeds` are plain address strings here, parsed
/// into [`HwAddress`] by [`Self::to_power_gate`], the same split as
/// [`HardwareResourceFile`]/[`HwResource`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct HardwarePowerGateFile {
    pub gpio: String,
    pub active_level: HwGateLevel,
    #[serde(default)]
    pub open_drain: bool,
    pub settle_ms: u32,
    #[serde(default = "default_off_debounce_ms")]
    pub off_debounce_ms: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub feeds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl HardwarePowerGateFile {
    fn to_power_gate(&self) -> Result<HwPowerGate, HardwareManifestFileError> {
        let gpio = HwAddress::new(self.gpio.clone())?;
        let feeds = self
            .feeds
            .iter()
            .cloned()
            .map(HwAddress::new)
            .collect::<Result<Vec<_>, _>>()?;
        let mut gate = HwPowerGate::new(gpio, self.active_level, self.settle_ms)
            .with_open_drain(self.open_drain)
            .with_off_debounce_ms(self.off_debounce_ms)
            .with_feeds(feeds);
        if let Some(note) = &self.note {
            gate = gate.with_note(note.clone());
        }
        Ok(gate)
    }
}

/// `off_debounce_ms` default for a manifest file that omits it. Ties to
/// [`HwPowerGate::DEFAULT_OFF_DEBOUNCE_MS`] so the file-format default and the
/// runtime default can never drift apart.
fn default_off_debounce_ms() -> u32 {
    HwPowerGate::DEFAULT_OFF_DEBOUNCE_MS
}

/// Errors produced while parsing, validating, or converting a manifest file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareManifestFileError {
    Parse { message: String },
    Serialize { message: String },
    Invalid { message: String },
    Hardware(HwError),
}

impl fmt::Display for HardwareManifestFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse { message } => write!(f, "manifest parse error: {message}"),
            Self::Serialize { message } => write!(f, "manifest serialize error: {message}"),
            Self::Invalid { message } => write!(f, "invalid manifest: {message}"),
            Self::Hardware(error) => write!(f, "{error}"),
        }
    }
}

impl From<HwError> for HardwareManifestFileError {
    fn from(error: HwError) -> Self {
        Self::Hardware(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_converts_manifest_file() {
        let manifest = HardwareManifestFile::read_json(
            r#"{
  "id": "seeed/xiao-esp32-c6",
  "target": "esp32c6",
  "vendor": "seeed",
  "product": "XIAO ESP32-C6",
  "description": "Seeed Studio XIAO ESP32-C6 board profile.",
  "url": "https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html",
  "gpio": [
    {
      "address": "/gpio/18",
      "display_label": "D6",
      "capabilities": ["gpio-output", "gpio-input"],
      "aliases": ["GPIO18", "IO18"]
    }
  ]
}"#,
        )
        .unwrap();

        let runtime = manifest.to_manifest().unwrap();

        assert_eq!(runtime.board_id(), "seeed/xiao-esp32-c6");
        assert_eq!(runtime.target(), Some(HardwareTarget::Esp32c6));
        assert_eq!(runtime.vendor(), Some("seeed"));
        assert_eq!(runtime.product(), Some("XIAO ESP32-C6"));
        assert!(runtime.resource(&HwAddress::gpio(18)).is_some());
    }

    #[test]
    fn rejects_duplicate_resource_addresses() {
        let manifest = HardwareManifestFile {
            id: "board".into(),
            target: HardwareTarget::Esp32c6,
            vendor: "vendor".into(),
            product: "product".into(),
            description: None,
            url: None,
            soft_limits: None,
            board_label: Vec::new(),
            gpio: alloc::vec![
                HardwareResourceFile::new("/gpio/1", "GPIO1", [HwCapability::GpioOutput]),
                HardwareResourceFile::new("/gpio/1", "GPIO1", [HwCapability::GpioInput]),
            ],
            resource: Vec::new(),
            power_gate: Vec::new(),
        };

        assert!(manifest.validate().is_err());
    }

    /// Soft limits survive the JSON round trip and the runtime conversion,
    /// and an absent field parses (older manifests stay valid).
    #[test]
    fn soft_limits_round_trip_and_default_to_absent() {
        let json = r#"{
            "id": "vendor/board",
            "target": "esp32",
            "vendor": "vendor",
            "product": "board",
            "soft_limits": {
                "totalLeds": { "value": 1500, "measured": "2026-08-05 soak" }
            }
        }"#;
        let file = HardwareManifestFile::read_json(json).unwrap();
        let limit = file
            .soft_limits
            .as_ref()
            .and_then(|limits| limits.total_leds.as_ref())
            .expect("the record must parse");
        assert_eq!(limit.value, 1500);
        assert_eq!(limit.measured, "2026-08-05 soak");

        let runtime = file.to_manifest().unwrap();
        assert_eq!(
            runtime
                .soft_limits()
                .and_then(|limits| limits.total_leds.as_ref())
                .map(|limit| limit.value),
            Some(1500),
        );

        let rewritten = file.write_json().unwrap();
        assert_eq!(HardwareManifestFile::read_json(&rewritten).unwrap(), file);

        let without = r#"{
            "id": "vendor/board",
            "target": "esp32",
            "vendor": "vendor",
            "product": "board"
        }"#;
        let file = HardwareManifestFile::read_json(without).unwrap();
        assert!(file.soft_limits.is_none(), "absence must stay valid");
    }

    /// One power gate survives the JSON round trip and the runtime
    /// conversion, every field lands, and an absent block parses to an empty
    /// slice (older manifests stay valid).
    #[test]
    fn power_gate_round_trip_and_default_to_absent() {
        let json = r#"{
            "id": "quinled/dig2go",
            "target": "esp32",
            "vendor": "quinled",
            "product": "dig2go",
            "power_gate": [
                {
                    "gpio": "/gpio/12",
                    "active_level": "low",
                    "open_drain": false,
                    "settle_ms": 50,
                    "off_debounce_ms": 5000,
                    "feeds": ["/gpio/16"],
                    "note": "MTDI strap; must idle low at boot"
                }
            ]
        }"#;
        let file = HardwareManifestFile::read_json(json).unwrap();
        assert_eq!(file.power_gate.len(), 1);
        let gate = &file.power_gate[0];
        assert_eq!(gate.gpio, "/gpio/12");
        assert_eq!(gate.active_level, HwGateLevel::Low);
        assert!(!gate.open_drain);
        assert_eq!(gate.settle_ms, 50);
        assert_eq!(gate.off_debounce_ms, 5000);
        assert_eq!(gate.feeds, alloc::vec![String::from("/gpio/16")]);
        assert_eq!(
            gate.note.as_deref(),
            Some("MTDI strap; must idle low at boot")
        );

        let runtime = file.to_manifest().unwrap();
        let gates = runtime.power_gates();
        assert_eq!(gates.len(), 1);
        assert_eq!(gates[0].gpio(), &HwAddress::gpio(12));
        assert_eq!(gates[0].active_level(), HwGateLevel::Low);
        assert!(!gates[0].open_drain());
        assert_eq!(gates[0].settle_ms(), 50);
        assert_eq!(gates[0].off_debounce_ms(), 5000);
        assert_eq!(gates[0].feeds(), [HwAddress::gpio(16)]);
        assert_eq!(gates[0].note(), Some("MTDI strap; must idle low at boot"));

        let rewritten = file.write_json().unwrap();
        assert_eq!(HardwareManifestFile::read_json(&rewritten).unwrap(), file);

        let without = r#"{
            "id": "vendor/board",
            "target": "esp32",
            "vendor": "vendor",
            "product": "board"
        }"#;
        let file = HardwareManifestFile::read_json(without).unwrap();
        assert!(file.power_gate.is_empty(), "absence must stay valid");
        assert!(file.to_manifest().unwrap().power_gates().is_empty());
    }

    /// `off_debounce_ms` and `open_drain` are the two fields with defaults —
    /// omitting them must not fail parsing, and must land on the documented
    /// defaults (5000 ms, not open-drain).
    #[test]
    fn power_gate_off_debounce_and_open_drain_default() {
        let json = r#"{
            "id": "vendor/board",
            "target": "esp32",
            "vendor": "vendor",
            "product": "board",
            "power_gate": [
                { "gpio": "/gpio/12", "active_level": "high", "settle_ms": 20 }
            ]
        }"#;
        let file = HardwareManifestFile::read_json(json).unwrap();
        let gate = &file.power_gate[0];
        assert_eq!(gate.off_debounce_ms, 5000);
        assert!(!gate.open_drain);
        assert_eq!(gate.off_debounce_ms, HwPowerGate::DEFAULT_OFF_DEBOUNCE_MS);
    }

    /// An invalid gpio path in a gate entry fails `to_manifest()` with a
    /// clear error — the same treatment as an invalid resource address.
    #[test]
    fn rejects_invalid_power_gate_gpio() {
        let manifest = HardwareManifestFile {
            id: "board".into(),
            target: HardwareTarget::Esp32,
            vendor: "vendor".into(),
            product: "product".into(),
            description: None,
            url: None,
            soft_limits: None,
            board_label: Vec::new(),
            gpio: Vec::new(),
            resource: Vec::new(),
            power_gate: alloc::vec![HardwarePowerGateFile {
                gpio: "gpio/12".into(), // missing the leading slash
                active_level: HwGateLevel::High,
                open_drain: false,
                settle_ms: 20,
                off_debounce_ms: 5000,
                feeds: Vec::new(),
                note: None,
            }],
        };

        assert!(manifest.validate().is_err());
        assert!(manifest.to_manifest().is_err());
    }
}
