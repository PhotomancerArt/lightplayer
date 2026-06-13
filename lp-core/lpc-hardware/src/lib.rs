//! Hardware discovery, ownership, and driver contracts.
//!
//! `lpc-hardware` describes the board-facing side of LightPlayer without tying
//! it to one firmware target. A [`HwManifest`] lists concrete [`HwResource`]s
//! such as GPIO pins, RMT channels, and radios. A [`HwRegistry`] owns the live
//! claim/lease state for those resources so independent drivers cannot open the
//! same pin or peripheral at the same time.
//!
//! Drivers expose user-facing [`HwEndpoint`]s from those resources. The
//! [`HardwareSystem`] is the small router that collects registered drivers,
//! lists endpoints, and opens an endpoint by authored [`HwEndpointSpec`],
//! internal [`HwEndpointId`], or physical [`HwAddress`].
//!
//! Rendering and protocol-adjacent color processing live above this crate. For
//! example, [`Ws281xOutput`] accepts already-rendered RGB bytes; display
//! pipeline options remain in `lpc-shared`.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod drivers;
pub mod endpoint;
pub mod hw_error;
pub mod hw_system;
pub mod manifest;
pub mod output_error;
pub mod registry;
pub mod resource;

pub use output_error::OutputError;

pub use drivers::button::button_debouncer::ButtonDebouncer;
pub use drivers::button::button_driver::{ButtonConfig, ButtonDriver, ButtonInput};
pub use drivers::button::button_event::{ButtonEvent, ButtonEventKind};
pub use drivers::button::virtual_button::VirtualButton;
pub use drivers::button::virtual_button_driver::VirtualButtonDriver;
pub use drivers::hw_driver::HwDriver;
pub use drivers::radio::radio_channel::{
    RadioChannelId, RadioDeviceId, RadioDrainReport, RadioEventId,
};
pub use drivers::radio::radio_driver::{RadioConfig, RadioDevice, RadioDriver};
pub use drivers::radio::radio_message::{
    RADIO_MAX_PACKET_LEN, RADIO_MAX_PAYLOAD_LEN, RADIO_WIRE_HEADER_LEN, RADIO_WIRE_MAGIC,
    RADIO_WIRE_VERSION, RadioMessage, RadioMessageKind, RadioPacketError,
};
pub use drivers::radio::virtual_radio_driver::VirtualRadioDriver;
pub use drivers::ws281x::virtual_ws281x_driver::{VirtualWs281xDriver, VirtualWs281xOutput};
pub use drivers::ws281x::ws281x_driver::{Ws281xConfig, Ws281xDriver, Ws281xOutput};
pub use endpoint::hw_endpoint::HwEndpoint;
pub use endpoint::hw_endpoint_error::HardwareEndpointError;
pub use endpoint::hw_endpoint_id::HwEndpointId;
pub use endpoint::hw_endpoint_kind::HwEndpointKind;
pub use endpoint::hw_endpoint_status::HwEndpointStatus;
pub use hw_error::HwError;
pub use hw_system::HardwareSystem;
pub use lpc_model::HwEndpointSpec;
pub use manifest::default_manifests::{
    default_esp32c6_hardware_manifest, permissive_emu_hardware_manifest,
};
pub use manifest::hw_manifest::HwManifest;
pub use manifest::hw_manifest_file::{
    HardwareBoardLabelFile, HardwareBoardLabelStatus, HardwareManifestFile,
    HardwareManifestFileError,
};
pub use manifest::hw_target::HardwareTarget;
pub use registry::hw_claim::HwClaim;
pub use registry::hw_lease::{HardwareLease, HwLeaseId};
pub use registry::hw_registry::HwRegistry;
pub use resource::hw_address::HwAddress;
pub use resource::hw_capability::HwCapability;
pub use resource::hw_resource::HwResource;
