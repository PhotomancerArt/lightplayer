//! Resolver module — runtime data types for slot resolution.
//!
//! This module provides the data structures used by the M4.3 resolver
//! to cache resolved slot values per `NodeEntry`. It does NOT include
//! the resolver algorithm itself (that's M4.3 work).

pub mod binding_kind;
pub mod resolve_source;
pub mod resolved_slot;
pub mod resolver_cache;

pub use binding_kind::BindingKind;
pub use resolve_source::ResolveSource;
pub use resolved_slot::ResolvedSlot;
pub use resolver_cache::ResolverCache;
