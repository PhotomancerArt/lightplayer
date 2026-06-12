//! Hardware capabilities, manifests, endpoint routing, and driver traits.

#![no_std]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod display_pipeline_options;
pub mod driver;
pub mod drivers;
pub mod endpoint;
pub mod hardware_error;
pub mod hardware_system;
pub mod manifest;
pub mod output_error;
pub mod registry;
pub mod resource;

pub use display_pipeline_options::DisplayPipelineOptions;
pub use output_error::OutputError;

pub use driver::hardware_driver::HardwareDriver;
pub use drivers::button::button_debouncer::ButtonDebouncer;
pub use drivers::button::button_driver::{ButtonConfig, ButtonDriver, ButtonInput};
pub use drivers::button::button_event::{ButtonEvent, ButtonEventKind};
pub use drivers::button::virtual_button::VirtualButton;
pub use drivers::button::virtual_button_driver::VirtualButtonDriver;
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
pub use endpoint::hardware_endpoint::HardwareEndpoint;
pub use endpoint::hardware_endpoint_error::HardwareEndpointError;
pub use endpoint::hardware_endpoint_id::HardwareEndpointId;
pub use endpoint::hardware_endpoint_kind::HardwareEndpointKind;
pub use endpoint::hardware_endpoint_status::HardwareEndpointStatus;
pub use hardware_error::HardwareError;
pub use hardware_system::HardwareSystem;
pub use lpc_model::HardwareEndpointSpec;
pub use manifest::default_manifests::default_esp32c6_hardware_manifest;
pub use manifest::hardware_manifest::HardwareManifest;
pub use manifest::hardware_manifest_file::{
    HardwareBoardLabelFile, HardwareBoardLabelStatus, HardwareManifestFile,
    HardwareManifestFileError,
};
pub use manifest::hardware_target::HardwareTarget;
pub use registry::hardware_claim::HardwareClaim;
pub use registry::hardware_lease::{HardwareLease, HardwareLeaseId};
pub use registry::hardware_registry::HardwareRegistry;
pub use resource::hardware_address::HardwareAddress;
pub use resource::hardware_capability::HardwareCapability;
pub use resource::hardware_resource::HardwareResource;
