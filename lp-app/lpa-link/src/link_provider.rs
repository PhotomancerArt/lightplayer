use crate::link_endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::{LinkEndpoint, LinkError, LinkSession};
use serde::{Deserialize, Serialize};

#[allow(async_fn_in_trait, reason = "Link providers are not object-safe yet")]
pub trait LinkProvider {
    /// Provider-specific live session type.
    type Session: LinkSession;

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
    /// The returned session owns the live resource. Use `LinkSession::connection()`
    /// when the caller needs the `lp-server` protocol handoff.
    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<Self::Session, LinkError>;
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
