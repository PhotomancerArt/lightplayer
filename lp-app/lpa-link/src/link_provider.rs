use crate::{
    LinkEndpoint, LinkEndpointId, LinkEndpointStatus, LinkError, LinkProviderId, LinkSession,
};

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
