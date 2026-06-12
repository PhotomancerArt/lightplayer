//! Openable hardware endpoints derived from manifest resources.
//!
//! Endpoints are the bridge between authored specs such as `ws281x:rmt:D10` and
//! the lower-level [`crate::HwAddress`] resources that drivers claim. A driver
//! reports endpoint status from the [`crate::HwRegistry`] so callers can see
//! whether an endpoint is available, reserved, or already in use.

pub mod hw_endpoint;
pub mod hw_endpoint_error;
pub mod hw_endpoint_id;
pub mod hw_endpoint_kind;
pub mod hw_endpoint_status;
