use alloc::string::String;
use alloc::vec::Vec;

use crate::{HardwareAddress, HardwareCapability};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareResource {
    address: HardwareAddress,
    capabilities: Vec<HardwareCapability>,
    display_label: String,
    aliases: Vec<String>,
    location: Option<String>,
    reserved_reason: Option<String>,
}

impl HardwareResource {
    pub fn new(
        address: HardwareAddress,
        capabilities: impl Into<Vec<HardwareCapability>>,
        display_label: impl Into<String>,
    ) -> Self {
        Self {
            address,
            capabilities: capabilities.into(),
            display_label: display_label.into(),
            aliases: Vec::new(),
            location: None,
            reserved_reason: None,
        }
    }

    pub fn with_aliases(mut self, aliases: impl Into<Vec<String>>) -> Self {
        self.aliases = aliases.into();
        self
    }

    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    pub fn reserved(mut self, reason: impl Into<String>) -> Self {
        self.reserved_reason = Some(reason.into());
        self
    }

    pub fn address(&self) -> &HardwareAddress {
        &self.address
    }

    pub fn capabilities(&self) -> &[HardwareCapability] {
        &self.capabilities
    }

    pub fn display_label(&self) -> &str {
        &self.display_label
    }

    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    pub fn location(&self) -> Option<&str> {
        self.location.as_deref()
    }

    pub fn reserved_reason(&self) -> Option<&str> {
        self.reserved_reason.as_deref()
    }

    pub fn supports(&self, capability: HardwareCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}
