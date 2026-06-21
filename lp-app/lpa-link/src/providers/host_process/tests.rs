use crate::providers::host_process::HostProcessProvider;
use crate::{LinkEndpointId, LinkProvider, LinkSession};

#[tokio::test]
async fn host_process_connection_serves_client_requests() {
    let mut provider = provider_with_two_endpoints();
    let endpoint_id = LinkEndpointId::new("host-process-memory-1");
    let mut session = provider.connect(&endpoint_id).await.unwrap();

    let connection = session.connection().await.unwrap();
    assert!(matches!(
        connection.kind,
        crate::LinkConnectionKind::HostProcess
    ));
    let client = connection.server_client().unwrap();
    let projects = client.project_list_available().await.unwrap();

    assert!(projects.is_empty());
    session.close().await.unwrap();
}

#[tokio::test]
async fn host_process_provider_supports_multiple_endpoints() {
    let mut provider = provider_with_two_endpoints();
    let endpoints = provider.discover().await.unwrap();

    assert_eq!(endpoints.len(), 2);

    let mut session_a = provider.connect(&endpoints[0].id).await.unwrap();
    let mut session_b = provider.connect(&endpoints[1].id).await.unwrap();

    assert_ne!(session_a.id(), session_b.id());
    assert_ne!(session_a.endpoint_id(), session_b.endpoint_id());

    session_a.close().await.unwrap();
    session_b.close().await.unwrap();
}

fn provider_with_two_endpoints() -> HostProcessProvider {
    let mut provider = HostProcessProvider::new("host-process");
    provider.create_memory_endpoint("Host Process A");
    provider.create_memory_endpoint("Host Process B");
    provider
}
