//! Resolver module — runtime data types and binding cascade resolution.
//!
//! This module provides:
//! - Data structures for caching resolved slot values (`ResolverCache`, `ResolvedSlot`)
//! - Binding cascade resolution (`resolve_slot`)
//! - Context facade for resolver access (`ResolverContext`)
//! - Error types for resolution failures (`ResolveError`)

pub mod binding_kind;
pub mod resolve_error;
pub mod resolve_source;
pub mod resolved_slot;
pub mod resolver;
pub mod resolver_cache;
pub mod resolver_context;

pub use binding_kind::BindingKind;
pub use resolve_error::ResolveError;
pub use resolve_source::ResolveSource;
pub use resolved_slot::ResolvedSlot;
pub use resolver::{resolve_slot, resolve_slot_owned};
pub use resolver_cache::ResolverCache;
pub use resolver_context::ResolverContext;
