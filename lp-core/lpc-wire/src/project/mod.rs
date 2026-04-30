//! Wire-facing project types (`Wire*` where applicable).

mod wire_project_handle;
mod wire_project_request;

pub use wire_project_handle::WireProjectHandle;
pub use wire_project_request::{ApiNodeSpecifier, WireNodeStatus, WireProjectRequest};
