//! Places: everywhere a project can live (roadmap D18/D19).
//!
//! The library is the source of truth; runtimes (simulator now, devices in
//! M5) are places projects are pushed to and pulled from. The trait here is
//! deliberately small — it establishes the seam (kind + capacity) that M4's
//! gallery and M5's device flows grow against; the ops live on the concrete
//! types until real callers shape the abstraction.

pub mod device_identity;
pub mod device_registry;
pub mod place;
pub mod runtime_place;

pub use device_identity::{DEVICE_IDENTITY_PATH, DeviceIdentity};
pub use device_registry::{DeviceRegistry, RegisteredDevice};
pub use place::{Place, PlaceDescriptor, PlaceKind};
pub use runtime_place::{RuntimePlace, relate_runtime_content};
