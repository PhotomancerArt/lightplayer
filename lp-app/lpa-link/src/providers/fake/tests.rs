use crate::provider::endpoint::LinkEndpointId;
use crate::providers::LinkProviderKind;
use crate::providers::fake::FakeProvider;
use crate::{LinkCapabilities, LinkEndpoint, LinkError, LinkManagementRequest, LinkProvider};

#[tokio::test]
async fn discover_returns_all_fake_endpoints() {
    let mut provider = fake_provider();

    let endpoints = provider.discover().await.unwrap();

    assert_eq!(endpoints.len(), 2);
    assert_eq!(endpoints[0].id.as_str(), "fake-a");
    assert_eq!(endpoints[1].id.as_str(), "fake-b");
}

#[tokio::test]
async fn sessions_are_scoped_to_endpoint_and_have_stable_ids() {
    let mut provider = fake_provider();
    let endpoint_a = LinkEndpointId::new("fake-a");
    let endpoint_b = LinkEndpointId::new("fake-b");

    let session_a = provider.connect(&endpoint_a).await.unwrap();
    let session_b = provider.connect(&endpoint_b).await.unwrap();

    assert_eq!(session_a.endpoint_id().as_str(), "fake-a");
    assert_eq!(session_b.endpoint_id().as_str(), "fake-b");
    assert_ne!(session_a.id(), session_b.id());

    let connection = provider.connection(session_a.id()).await.unwrap();
    assert_eq!(connection.endpoint_id.as_str(), "fake-a");
    assert_eq!(connection.session_id, session_a.id().clone());
}

#[tokio::test]
async fn logs_and_diagnostics_are_scoped_to_session() {
    let mut provider = fake_provider();
    let session = provider
        .connect(&LinkEndpointId::new("fake-a"))
        .await
        .unwrap();

    let logs = provider.logs(session.id()).unwrap();
    let diagnostics = provider.diagnostics(session.id()).unwrap();

    assert_eq!(logs[0].endpoint_id.as_str(), "fake-a");
    assert_eq!(logs[0].session_id, Some(session.id().clone()));
    assert_eq!(diagnostics[0].endpoint_id.as_str(), "fake-a");
    assert_eq!(diagnostics[0].session_id, Some(session.id().clone()));

    provider.close(session.id()).await.unwrap();
    assert!(provider.connection(session.id()).await.is_err());
}

#[tokio::test]
async fn unsupported_management_request_returns_link_error() {
    let mut provider = fake_provider();
    let session = provider
        .connect(&LinkEndpointId::new("fake-a"))
        .await
        .unwrap();

    let error = provider
        .manage(session.id(), LinkManagementRequest::FlashFirmware)
        .await
        .unwrap_err();

    assert!(matches!(error, LinkError::OperationUnsupported { .. }));
}

fn fake_provider() -> FakeProvider {
    FakeProvider::new()
        .with_endpoint(
            LinkEndpoint::new("fake-a", LinkProviderKind::Fake, "Fake A")
                .with_capabilities(LinkCapabilities::diagnostics_only()),
        )
        .with_endpoint(LinkEndpoint::new(
            "fake-b",
            LinkProviderKind::Fake,
            "Fake B",
        ))
}
