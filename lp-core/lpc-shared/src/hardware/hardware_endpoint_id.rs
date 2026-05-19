use alloc::format;
use alloc::string::String;
use core::fmt;

use super::HardwareAddress;
use lpc_model::HardwareEndpointSpec;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HardwareEndpointId(String);

impl HardwareEndpointId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn for_driver_address(driver_id: &str, address: &HardwareAddress) -> Self {
        Self(format!("{driver_id}:{}", address.as_str()))
    }

    pub fn for_driver_spec(driver_id: &str, spec: &HardwareEndpointSpec) -> Self {
        Self(format!("{driver_id}:{}", spec.as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HardwareEndpointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
