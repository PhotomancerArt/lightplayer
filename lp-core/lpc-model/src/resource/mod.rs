//! Runtime payload resources referenced by id.
//!
//! Resources are engine-owned payload objects that can be summarized or fetched
//! over the wire. Slot shapes also live in a registry, but they are metadata,
//! not resources in this sense.

pub mod render_product;
pub mod render_product_id;
pub mod resource_domain;
pub mod resource_ref;
pub mod runtime_buffer_id;

pub use render_product::RenderProduct;
pub use render_product_id::RenderProductId;
pub use resource_domain::ResourceDomain;
pub use resource_ref::ResourceRef;
pub use runtime_buffer_id::RuntimeBufferId;
