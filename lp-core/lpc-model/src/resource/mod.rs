//! Store-backed payload resources referenced by id.
//!
//! Resources are engine-owned payload objects that can be summarized or fetched
//! over the wire. Slot shapes also live in a registry, but they are metadata,
//! not resources in this sense. Products are also not resources: they are lazy
//! graph values that ask a node to materialize into a caller-owned target.

pub mod resource_domain;
pub mod resource_ref;

pub use crate::resources::buffer::RuntimeBufferId;
pub use resource_domain::ResourceDomain;
pub use resource_ref::ResourceRef;
