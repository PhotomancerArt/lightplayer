//! Places: everywhere a project can live (roadmap D18/D19).
//!
//! The library is the source of truth; runtimes (simulator and devices)
//! are places projects are pushed to and pulled from. The trait here is
//! deliberately small — it establishes the seam (kind + capacity) — and
//! the ops live on the concrete types until real callers shape the
//! abstraction (`RuntimePlace` still has none; see its module doc and
//! the runtime-pool ADR).
//!
//! What survives the device-system teardown (M2 of the device-model
//! rebuild) is the durable half: the device REGISTRY (the remembered-board
//! record store and its on-disk format) and [`HardwareId`], the canonical
//! identity format that store persists. The connect-as-pull machinery and
//! the connect-time identity resolution went with the old device flows —
//! the rebuilt model owns those.

pub mod device_registry;
pub mod hardware_id;
pub mod place;
pub mod runtime_place;

pub use device_registry::{DeviceRegistry, RegisteredDevice};
pub use hardware_id::{HardwareId, HardwareIdParseError};
pub use place::{Place, PlaceDescriptor, PlaceKind};
pub use runtime_place::{RuntimePlace, relate_runtime_content};
