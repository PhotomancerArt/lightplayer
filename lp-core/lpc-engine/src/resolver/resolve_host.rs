//! [`ResolveHost`] — callback for uncached [`crate::resolver::QueryKey::NodeOutput`] (and
//! unbound [`crate::resolver::QueryKey::NodeInput`]) production.

use crate::resolver::produced_value::ProducedValue;
use crate::resolver::query_key::QueryKey;
use crate::resolver::resolve_error::SessionResolveError;
use crate::resolver::resolve_session::ResolveSession;

/// Engine or test fake that can satisfy demand for uncached queries.
pub trait ResolveHost {
    fn produce(
        &mut self,
        query: &QueryKey,
        session: &mut ResolveSession<'_>,
    ) -> Result<ProducedValue, SessionResolveError>;
}
