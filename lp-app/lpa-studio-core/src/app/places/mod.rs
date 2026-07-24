//! Places: everywhere a project can live (roadmap D18/D19).
//!
//! The library is the source of truth; runtimes (simulator and devices)
//! are places projects are pushed to and pulled from. The trait here is
//! deliberately small — it establishes the seam (kind + capacity) — and
//! the ops live on the concrete types until real callers shape the
//! abstraction (`RuntimePlace` still has none; see its module doc and
//! the runtime-pool ADR). The load-bearing content of this module is the
//! connect-as-pull machinery (`device_session`), the device registry,
//! and device identity.

pub mod device_identity;
pub mod device_registry;
pub mod device_session;
pub mod place;
pub mod runtime_place;

pub use device_identity::{DEVICE_IDENTITY_PATH, DeviceIdentity};
pub use device_registry::{DeviceRegistry, RegisteredDevice};
pub use device_session::{
    DeviceContent, DeviceSyncState, PulledDeviceCopy, pull_device_copy, registry_entry_for,
};
pub use place::{Place, PlaceDescriptor, PlaceKind};
pub use runtime_place::{RuntimePlace, relate_runtime_content};
