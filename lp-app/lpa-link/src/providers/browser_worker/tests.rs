use crate::providers::browser_worker::{
    BrowserInputEnvelope, BrowserRuntimeOptions, BrowserRuntimeTier, BrowserTickMode,
    BrowserWorkerProvider,
};
use crate::{LinkConnectionKind, LinkProvider};

/// A runtime is a runtime however it came to exist: `Boot` and
/// `CreateRuntime` carry the SAME options payload, and the board inside it
/// crosses as opaque text.
#[test]
fn boot_and_create_runtime_carry_the_same_runtime_options() {
    let manifest = r#"{"id":"lightplayer/desktop","target":"desktop"}"#;
    let runtime = BrowserRuntimeOptions::new(BrowserRuntimeTier::Gpu, manifest)
        .with_identity("02:00:00:ab:cd:ef");

    let boot = serde_json::to_value(BrowserInputEnvelope::Boot {
        label: "studio".to_string(),
        fw_browser_module_path: "/pkg/fw_browser.js".to_string(),
        fw_browser_wasm_path: "/pkg/fw_browser_bg.wasm".to_string(),
        tick_mode: BrowserTickMode::SelfTicking,
        module_delivery: "path".to_string(),
        runtime: runtime.clone(),
    })
    .expect("boot envelope");
    let created = serde_json::to_value(BrowserInputEnvelope::CreateRuntime {
        label: "preview-slot-1".to_string(),
        runtime,
    })
    .expect("create_runtime envelope");

    assert_eq!(boot["runtime"], created["runtime"]);
    assert_eq!(boot["runtime"]["tier"], "gpu");
    assert_eq!(boot["runtime"]["hardware_manifest_json"], manifest);
    assert_eq!(boot["runtime"]["identity"]["base_mac"], "02:00:00:ab:cd:ef");
}

/// No identity, no `identity` key — an unidentified runtime reports no MAC
/// rather than an empty one.
#[test]
fn runtime_options_omit_an_absent_identity() {
    let value = serde_json::to_value(BrowserRuntimeOptions::new(BrowserRuntimeTier::Cpu, "{}"))
        .expect("runtime options");

    assert_eq!(value.get("identity"), None);
    assert_eq!(value["tier"], "cpu");
}

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
