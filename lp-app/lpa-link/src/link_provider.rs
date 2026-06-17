use crate::{
    LinkEndpoint, LinkEndpointId, LinkEndpointStatus, LinkError, LinkProviderId, LinkSession,
};

#[allow(async_fn_in_trait, reason = "Link providers are not object-safe yet")]
pub trait LinkProvider {
    type Session: LinkSession;

    fn id(&self) -> &LinkProviderId;

    /// Discover endpoints currently offered by this provider.
    ///
    /// Providers may return physical endpoints, such as a future ESP32 serial
    /// port, or spawnable endpoints, such as `local-host` memory runtimes.
    async fn discover(&mut self) -> Result<Vec<LinkEndpoint>, LinkError>;

    async fn status(
        &mut self,
        endpoint_id: &LinkEndpointId,
    ) -> Result<LinkEndpointStatus, LinkError>;

    async fn connect(&mut self, endpoint_id: &LinkEndpointId) -> Result<Self::Session, LinkError>;
}
