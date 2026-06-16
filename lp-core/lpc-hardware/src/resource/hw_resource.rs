use alloc::string::String;
use alloc::vec::Vec;

use crate::{HwAddress, HwCapability};

/// One claimable resource in a board manifest.
///
/// A resource is addressed by [`HwAddress`], declares the capabilities drivers
/// may require, and carries human-facing labels/aliases from the board profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HwResource {
    address: HwAddress,
    capabilities: Vec<HwCapability>,
    display_label: String,
    aliases: Vec<String>,
    location: Option<String>,
    reserved_reason: Option<String>,
}

impl HwResource {
    pub fn new(
        address: HwAddress,
        capabilities: impl Into<Vec<HwCapability>>,
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

    pub fn clear_reservation(mut self) -> Self {
        self.reserved_reason = None;
        self
    }

    pub fn with_display_label(mut self, display_label: impl Into<String>) -> Self {
        self.display_label = display_label.into();
        self
    }

    pub fn address(&self) -> &HwAddress {
        &self.address
    }

    pub fn capabilities(&self) -> &[HwCapability] {
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

    pub fn supports(&self, capability: HwCapability) -> bool {
        self.capabilities.contains(&capability)
    }
}
