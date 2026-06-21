use crate::link_endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::link_session::LinkSessionId;
use crate::{LinkConnection, LinkDiagnostic, LinkEndpoint, LinkError, LinkLogEntry, LinkSession};
use serde::{Deserialize, Serialize};

#[allow(async_fn_in_trait, reason = "Link providers are not object-safe yet")]
pub trait LinkProvider {
    /// Stable provider id, such as `host-process` or `browser-serial-esp32`.
    fn id(&self) -> &LinkProviderId;

    /// Discover endpoints currently offered by this provider.
    ///
    /// Providers may return physical endpoints, such as an ESP32 serial port,
    /// or spawnable endpoints, such as `host-process` memory runtimes.
    async fn discover(&mut self) -> Result<Vec<LinkEndpoint>, LinkError>;

    async fn status(
        &mut self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<LinkEndpointStatus, LinkError>;

    /// Open a live session from a discovered endpoint.
    ///
    /// The provider owns the concrete resources behind the returned session
    /// record. Use the session id with `connection`, `logs`, `diagnostics`, and
    /// `close` for provider-owned follow-up operations.
    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<LinkSession, LinkError>;

    /// Open or return the client connection associated with a live session.
    async fn connection(&mut self, session_id: &LinkSessionId)
    -> Result<LinkConnection, LinkError>;

    /// Link-level logs available through the provider-owned session.
    fn logs(&self, session_id: &LinkSessionId) -> Result<Vec<LinkLogEntry>, LinkError>;

    /// Link-level diagnostics available through the provider-owned session.
    fn diagnostics(&self, session_id: &LinkSessionId) -> Result<Vec<LinkDiagnostic>, LinkError>;

    /// Close provider-owned resources for a live session.
    async fn close(&mut self, session_id: &LinkSessionId) -> Result<(), LinkError>;
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct LinkProviderId(String);

impl LinkProviderId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for LinkProviderId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for LinkProviderId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
