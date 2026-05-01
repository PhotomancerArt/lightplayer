//! Resolver module — runtime data types and binding cascade resolution.
//!
//! This module provides:
//! - Engine cache types (`ResolverCache` / [`QueryKey`] / [`Production`])
//! - Per-node slot cache ([`SlotResolverCache`], [`ResolvedSlot`]) for [`resolve_slot`]
//! - Binding cascade resolution (`resolve_slot`)
//! - Context facade for resolver access (`ResolverContext`)
//! - Error types for resolution failures (`ResolveError`, [`SessionResolveError`])
//! - Demand session ([`ResolveSession`], [`ResolveHost`]) and [`TickResolver`] bridge ([`SessionHostResolver`])

pub mod binding_kind;
pub mod production;
pub mod query_key;
pub mod resolve_error;
pub mod resolve_host;
pub mod resolve_session;
pub mod resolve_source;
pub mod resolve_trace;
pub mod resolved_slot;
pub mod resolver;
pub mod resolver_cache;
pub mod resolver_context;
pub mod slot_resolver_cache;
pub mod tick_resolver;

pub use binding_kind::BindingKind;
pub use production::{Production, ProductionSource};
pub use query_key::QueryKey;
pub use resolve_error::{ResolveError, SessionResolveError};
pub use resolve_host::ResolveHost;
pub use resolve_session::ResolveSession;
pub use resolve_source::ResolveSource;
pub use resolve_trace::{
    ResolveLogLevel, ResolveTrace, ResolveTraceError, ResolveTraceEvent, TraceGuard,
};
pub use resolved_slot::ResolvedSlot;
pub use resolver::{Resolver, resolve_slot, resolve_slot_owned};
pub use resolver_cache::ResolverCache;
pub use resolver_context::ResolverContext;
pub use slot_resolver_cache::SlotResolverCache;
pub use tick_resolver::{SessionHostResolver, TickResolver};
