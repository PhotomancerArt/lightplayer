use crate::providers::browser_serial_esp32::{
    BrowserSerialEsp32Provider, DEFAULT_ESPTOOL_MODULE_PATH,
};
use crate::{LinkConnectionKind, LinkOperation, LinkProvider};

#[tokio::test]
async fn browser_serial_provider_models_granted_ports() {
    let mut provider = BrowserSerialEsp32Provider::new();
    let endpoint_id = provider.create_granted_endpoint("ESP32-C6", 7);

    let endpoints = provider.discover().await.unwrap();

    assert_eq!(endpoints.len(), 1);
    assert_eq!(endpoints[0].id, endpoint_id);
    assert!(endpoints[0].capabilities.supports(LinkOperation::Reset));
    assert!(endpoints[0].capabilities.supports(LinkOperation::ReadLogs));
    assert!(
        endpoints[0]
            .capabilities
            .supports(LinkOperation::FlashFirmware)
    );
    assert!(
        endpoints[0]
            .capabilities
            .supports(LinkOperation::EraseDeviceFlash)
    );
}

#[tokio::test]
async fn browser_serial_connection_reports_protocol() {
    let mut provider = BrowserSerialEsp32Provider::new();
    let endpoint_id = provider.create_granted_endpoint("ESP32-C6", 7);
    let session = provider.connect(&endpoint_id).await.unwrap();

    let connection = provider.connection(session.id()).await.unwrap();

    assert!(matches!(
        connection.kind,
        LinkConnectionKind::BrowserSerialEsp32 { ref protocol }
            if protocol == "lp-serial-json-lines-v1"
    ));
}

#[test]
fn default_esptool_module_path_uses_browser_esm_with_json_named_exports() {
    assert_eq!(
        BrowserSerialEsp32Provider::new()
            .options()
            .esptool_module_path
            .as_deref(),
        Some(DEFAULT_ESPTOOL_MODULE_PATH)
    );
    assert!(DEFAULT_ESPTOOL_MODULE_PATH.contains("cdn.jsdelivr.net/"));
    assert!(DEFAULT_ESPTOOL_MODULE_PATH.ends_with("/+esm"));
}
