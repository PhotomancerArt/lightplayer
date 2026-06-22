use crate::provider::endpoint::{LinkEndpointId, LinkEndpointStatus};
use crate::provider::management_request::LinkManagementRequest;
use crate::provider::management_result::LinkManagementResult;
use crate::provider::session::LinkSessionId;
use crate::providers::LinkProviderKind;
use crate::{LinkConnection, LinkDiagnostic, LinkEndpoint, LinkError, LinkLogEntry, LinkSession};

/// Controller interface for one built-in link provider.
///
/// A provider owns the resources for a single `LinkProviderKind`: discovered
/// endpoints, live sessions, serial ports, workers, spawned runtimes, and any
/// provider-specific management state. Callers hold lightweight endpoint and
/// session records and pass their ids back into the provider for follow-up
/// operations.
///
/// The trait is intentionally not used as a trait object today because async
/// trait methods are not object-safe. `LinkProviderRegistry` stores concrete
/// providers through `LinkProviderInstance`, an enum-dispatched wrapper that
/// still exposes this same controller interface.
#[allow(async_fn_in_trait, reason = "Link providers are not object-safe yet")]
pub trait LinkProvider {
    /// Stable built-in provider kind, such as `host-process` or `browser-serial-esp32`.
    fn kind(&self) -> LinkProviderKind;

    /// Discover endpoints currently offered by this provider.
    ///
    /// Providers may return physical endpoints, such as an ESP32 serial port,
    /// or spawnable endpoints, such as `host-process` memory runtimes.
    async fn discover(&mut self) -> Result<Vec<LinkEndpoint>, LinkError>;

    /// Return the current status for a previously discovered endpoint.
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

    /// Execute a low-level management operation through a live session.
    ///
    /// Providers that do not support the requested operation should return
    /// `LinkError::OperationUnsupported`. Management operations are below the
    /// `lp-server` protocol and may invalidate any server connection opened from
    /// the same session.
    async fn manage(
        &mut self,
        session_id: &LinkSessionId,
        request: LinkManagementRequest,
    ) -> Result<LinkManagementResult, LinkError> {
        let _ = session_id;
        Err(LinkError::unsupported(format!("{:?}", request.operation())))
    }

    /// Close provider-owned resources for a live session.
    async fn close(&mut self, session_id: &LinkSessionId) -> Result<(), LinkError>;
}
