//! Bus module — runtime registry of bus channels.
//!
//! This module provides the `Bus` container for managing channel
//! state at runtime. Channels are lazily created when first
/// referenced.
pub mod bus;
pub mod bus_error;
pub mod channel_entry;

pub use bus::Bus;
pub use bus_error::BusError;
pub use channel_entry::ChannelEntry;
