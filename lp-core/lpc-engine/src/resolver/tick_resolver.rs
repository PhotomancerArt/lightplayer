//! Node-facing demand resolution facade ([`TickResolver`]) backed by session + host.

use crate::resolver::produced_value::ProducedValue;
use crate::resolver::query_key::QueryKey;
use crate::resolver::resolve_error::{ResolveError, SessionResolveError};
use crate::resolver::resolve_host::ResolveHost;
use crate::resolver::resolve_session::ResolveSession;

/// Narrow API for [`crate::node::TickContext`] demand reads (`QueryKey` → [`ProducedValue`]).
pub trait TickResolver {
    fn resolve(&mut self, query: QueryKey) -> Result<ProducedValue, ResolveError>;
}

/// Bridges [`ResolveSession`] + [`ResolveHost`] into a [`TickResolver`].
///
/// `'resolver` is the session's resolver/registry borrow ([`ResolveSession`]'s lifetime parameter).
/// `'sess` is the borrow of that session from the caller (often shorter); splitting them avoids
/// invariant `'sess == 'resolver` churn when constructing from `&mut ResolveSession<'resolver>`.
pub struct SessionHostResolver<'sess, 'resolver, 'host> {
    pub session: &'sess mut ResolveSession<'resolver>,
    pub host: &'host mut dyn ResolveHost,
}

impl<'sess, 'resolver, 'host> TickResolver for SessionHostResolver<'sess, 'resolver, 'host> {
    fn resolve(&mut self, query: QueryKey) -> Result<ProducedValue, ResolveError> {
        self.session
            .resolve(self.host, query)
            .map_err(|e: SessionResolveError| ResolveError::new(alloc::format!("{e}")))
    }
}
