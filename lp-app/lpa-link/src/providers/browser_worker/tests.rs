use crate::providers::browser_worker::BrowserWorkerProvider;
use crate::{LinkConnectionKind, LinkProvider};

#[tokio::test]
async fn browser_worker_provider_supports_multiple_worker_endpoints() {
    let provider = BrowserWorkerProvider::new();
    provider.create_worker_endpoint("Browser A");
    provider.create_worker_endpoint("Browser B");

    let endpoints = provider.discover().await.unwrap();
    assert_eq!(endpoints.len(), 2);

    let session_a = provider.connect(&endpoints[0].id).await.unwrap();
    let session_b = provider.connect(&endpoints[1].id).await.unwrap();

    assert_ne!(session_a.id(), session_b.id());
    assert_ne!(session_a.endpoint_id(), session_b.endpoint_id());
}

#[tokio::test]
async fn browser_worker_connection_reports_worker_protocol() {
    let provider = BrowserWorkerProvider::new();
    let endpoint_id = provider.create_worker_endpoint("Browser A");
    let session = provider.connect(&endpoint_id).await.unwrap();

    let connection = provider.connection(session.id()).await.unwrap();

    assert_eq!(connection.endpoint_id, endpoint_id);
    assert!(matches!(
        connection.kind,
        LinkConnectionKind::BrowserWorker { ref protocol }
            if protocol == "fw-browser-post-message-v1"
    ));
}
