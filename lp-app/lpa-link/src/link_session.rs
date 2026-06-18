use crate::{
    LinkConnection, LinkDiagnostic, LinkEndpointId, LinkError, LinkLogEntry, LinkSessionId,
};

#[allow(async_fn_in_trait, reason = "Link sessions are not object-safe yet")]
pub trait LinkSession {
    fn id(&self) -> &LinkSessionId;

    fn endpoint_id(&self) -> &LinkEndpointId;

    fn logs(&self) -> Vec<LinkLogEntry>;

    fn diagnostics(&self) -> Vec<LinkDiagnostic>;

    /// Open or return the client connection associated with this session.
    ///
    /// The session owns lifecycle below the connection. For `host-process`, that
    /// means keeping the in-process `fw-host` runtime alive while the returned
    /// transport is in use.
    async fn connection(&mut self) -> Result<LinkConnection, LinkError>;

    async fn close(&mut self) -> Result<(), LinkError>;
}
