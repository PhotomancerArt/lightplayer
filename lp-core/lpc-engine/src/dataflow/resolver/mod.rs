//! Resolver module — runtime demand-resolution data types.
//!
//! This module provides:
//! - Engine cache types (`ResolverCache` / [`QueryKey`] / [`Production`])
//! - Error types for resolution failures (`ResolveError`, [`SessionResolveError`])
//! - Engine session ([`EngineSession`], [`ResolveHost`]) and [`TickResolver`] bridge ([`SessionHostResolver`])
//!
//! `Production` is aggregate-capable slot data. Leaf shader-compatible values
//! remain available through convenience helpers, but the resolver can also
//! carry maps, records, options, and receiver-owned merge results.

pub mod production;
pub mod query_key;
pub mod resolve_error;
pub mod resolve_host;
pub mod resolve_session;
pub mod resolve_trace;
pub mod resolver;
pub mod resolver_cache;
pub mod tick_resolver;

pub use production::{Production, ProductionSource};
pub use query_key::QueryKey;
pub use resolve_error::{ResolveError, SessionResolveError};
pub use resolve_host::ResolveHost;
pub use resolve_session::{EngineSession, ResolveSession};
pub use resolve_trace::{
    ResolveLogLevel, ResolveTrace, ResolveTraceError, ResolveTraceEvent, TraceGuard,
};
pub use resolver::Resolver;
pub use resolver_cache::ResolverCache;
pub use tick_resolver::{SessionHostResolver, TickResolver};
