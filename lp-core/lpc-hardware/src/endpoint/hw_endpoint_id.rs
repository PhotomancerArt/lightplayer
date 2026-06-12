use alloc::format;
use alloc::string::String;
use core::fmt;

use crate::HwAddress;
use lpc_model::HwEndpointSpec;

/// Internal endpoint identity scoped to the driver that exposes it.
///
/// Endpoint IDs are stable enough for routing inside [`crate::HardwareSystem`].
/// Authored project files should use [`HwEndpointSpec`] instead.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HwEndpointId(String);

impl HwEndpointId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn for_driver_address(driver_id: &str, address: &HwAddress) -> Self {
        Self(format!("{driver_id}:{}", address.as_str()))
    }

    pub fn for_driver_spec(driver_id: &str, spec: &HwEndpointSpec) -> Self {
        Self(format!("{driver_id}:{}", spec.as_str()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HwEndpointId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
