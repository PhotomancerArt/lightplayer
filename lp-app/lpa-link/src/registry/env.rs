/// Application-supplied resources needed to construct compiled-in providers.
///
/// `lpa-link` owns provider implementations, but some browser and host
/// providers need paths or options that belong to the embedding application:
/// bundled wasm modules, firmware manifests, esptool paths, serial options,
/// and similar deployment details. `LinkEnv` is the single feature-gated input
/// surface for those resources.
///
/// Fields appear only when their provider feature and target are enabled. A
/// host build with browser features, for example, does not expose browser env
/// fields because the browser providers themselves are not compiled on host.
#[derive(Clone, Default)]
pub struct LinkEnv {
    /// Browser Web Serial ESP32 provider options.
    #[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
    pub browser_serial_esp32: crate::providers::browser_serial_esp32::BrowserSerialEsp32Options,
    /// Browser worker provider options, including app-owned module/wasm paths.
    #[cfg(all(feature = "browser-worker", target_arch = "wasm32"))]
    pub browser_worker: crate::providers::browser_worker::BrowserWorkerOptions,
    /// Host serial ESP32 provider options.
    #[cfg(feature = "host-serial-esp32")]
    pub host_serial_esp32: crate::providers::host_serial_esp32::HostSerialEsp32Options,
}
