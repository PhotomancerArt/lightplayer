//! Runtime payload resources referenced by id.
//!
//! Resources are engine-owned payload objects that can be summarized or fetched
//! over the wire. Slot shapes also live in a registry, but they are metadata,
//! not resources in this sense.

pub mod control_product;
pub mod resource_domain;
pub mod resource_ref;
pub mod runtime_buffer_id;
pub mod visual_product;
pub mod visual_product_id;

pub use control_product::{ControlExtent, ControlProduct};
pub use resource_domain::ResourceDomain;
pub use resource_ref::ResourceRef;
pub use runtime_buffer_id::RuntimeBufferId;
pub use visual_product::VisualProduct;
pub use visual_product_id::VisualProductId;
