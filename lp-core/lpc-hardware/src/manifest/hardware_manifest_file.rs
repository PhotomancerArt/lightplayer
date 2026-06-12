use alloc::collections::BTreeSet;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    HardwareAddress, HardwareCapability, HardwareError, HardwareManifest, HardwareResource,
    HardwareTarget,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareManifestFile {
    pub id: String,
    pub target: HardwareTarget,
    pub vendor: String,
    pub product: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub board_label: Vec<HardwareBoardLabelFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gpio: Vec<HardwareResourceFile>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resource: Vec<HardwareResourceFile>,
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
            board_label: Vec::new(),
            gpio: Vec::new(),
            resource: Vec::new(),
        }
    }

    pub fn read_toml(toml_text: &str) -> Result<Self, HardwareManifestFileError> {
        toml::from_str(toml_text).map_err(|error| HardwareManifestFileError::Parse {
            message: error.to_string(),
        })
    }

    pub fn write_toml(&self) -> Result<String, HardwareManifestFileError> {
        toml::to_string_pretty(self).map_err(|error| HardwareManifestFileError::Serialize {
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

        let mut seen = BTreeSet::new();
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

        let mut seen = BTreeSet::new();
        for resource in self.gpio.iter().chain(self.resource.iter()) {
            let address = HardwareAddress::new(resource.address.clone())?;
            if !seen.insert(address.clone()) {
                return Err(HardwareManifestFileError::Invalid {
                    message: alloc::format!("duplicate resource address: {address}"),
                });
            }
        }
        Ok(())
    }

    pub fn to_manifest(&self) -> Result<HardwareManifest, HardwareManifestFileError> {
        self.validate()?;
        let resources = self.resources()?;
        let mut manifest = HardwareManifest::new(self.id.clone(), self.product.clone(), resources)
            .with_target(self.target)
            .with_vendor(self.vendor.clone())
            .with_product(self.product.clone());
        if let Some(description) = &self.description {
            manifest = manifest.with_description(description.clone());
        }
        if let Some(url) = &self.url {
            manifest = manifest.with_url(url.clone());
        }
        Ok(manifest)
    }

    fn resources(&self) -> Result<Vec<HardwareResource>, HardwareManifestFileError> {
        self.gpio
            .iter()
            .chain(self.resource.iter())
            .map(HardwareResourceFile::to_resource)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HardwareBoardLabelStatus {
    Unassigned,
    Assigned,
    Verified,
    NotFound,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwareResourceFile {
    pub address: String,
    pub display_label: String,
    pub capabilities: Vec<HardwareCapability>,
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
        capabilities: impl Into<Vec<HardwareCapability>>,
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

    fn to_resource(&self) -> Result<HardwareResource, HardwareManifestFileError> {
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
        let mut resource = HardwareResource::new(
            HardwareAddress::new(self.address.clone())?,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HardwareManifestFileError {
    Parse { message: String },
    Serialize { message: String },
    Invalid { message: String },
    Hardware(HardwareError),
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

impl From<HardwareError> for HardwareManifestFileError {
    fn from(error: HardwareError) -> Self {
        Self::Hardware(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_converts_manifest_file() {
        let manifest = HardwareManifestFile::read_toml(
            r#"
id = "seeed/xiao-esp32-c6"
target = "esp32c6"
vendor = "seeed"
product = "XIAO ESP32-C6"
description = "Seeed Studio XIAO ESP32-C6 board profile."
url = "https://www.seeedstudio.com/Seeed-Studio-XIAO-ESP32C6-p-5884.html"

[[gpio]]
address = "/gpio/18"
display_label = "D6"
capabilities = ["gpio-output", "gpio-input"]
aliases = ["GPIO18", "IO18"]
"#,
        )
        .unwrap();

        let runtime = manifest.to_manifest().unwrap();

        assert_eq!(runtime.board_id(), "seeed/xiao-esp32-c6");
        assert_eq!(runtime.target(), Some(HardwareTarget::Esp32c6));
        assert_eq!(runtime.vendor(), Some("seeed"));
        assert_eq!(runtime.product(), Some("XIAO ESP32-C6"));
        assert!(runtime.resource(&HardwareAddress::gpio(18)).is_some());
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
            board_label: Vec::new(),
            gpio: alloc::vec![
                HardwareResourceFile::new("/gpio/1", "GPIO1", [HardwareCapability::GpioOutput]),
                HardwareResourceFile::new("/gpio/1", "GPIO1", [HardwareCapability::GpioInput]),
            ],
            resource: Vec::new(),
        };

        assert!(manifest.validate().is_err());
    }
}
