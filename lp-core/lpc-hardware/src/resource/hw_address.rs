use alloc::format;
use alloc::string::String;
use core::fmt;

use crate::HwError;

/// Stable address for a concrete board resource.
///
/// Addresses identify physical or logical hardware resources inside a
/// [`crate::HwManifest`]. They are intentionally separate from user-facing
/// endpoint specs and board labels, which may vary by driver or board profile.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HwAddress(String);

impl HwAddress {
    pub fn new(path: impl Into<String>) -> Result<Self, HwError> {
        let path = path.into();
        validate_path(&path)?;
        Ok(Self(path))
    }

    pub fn gpio(pin: u32) -> Self {
        Self(format!("/gpio/{pin}"))
    }

    pub fn rmt_ws281x(channel: u8) -> Self {
        Self(format!("/rmt/ws281x{channel}"))
    }

    pub fn radio(index: u8) -> Self {
        Self(format!("/radio/{index}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HwAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

fn validate_path(path: &str) -> Result<(), HwError> {
    if !path.starts_with('/') || path.len() <= 1 {
        return Err(HwError::InvalidAddress {
            address: path.into(),
        });
    }
    if path.as_bytes().windows(2).any(|w| w == b"//") {
        return Err(HwError::InvalidAddress {
            address: path.into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_gpio_address() {
        assert_eq!(HwAddress::gpio(18).as_str(), "/gpio/18");
    }

    #[test]
    fn normalizes_radio_address() {
        assert_eq!(HwAddress::radio(0).as_str(), "/radio/0");
    }

    #[test]
    fn rejects_invalid_address() {
        assert!(HwAddress::new("gpio/18").is_err());
        assert!(HwAddress::new("/").is_err());
        assert!(HwAddress::new("/gpio//18").is_err());
    }
}
