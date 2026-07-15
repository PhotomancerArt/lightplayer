//! Manual hardware smoke for the host provider's espflash-lib `manage()`.
//!
//! Drives a real board through the M5 management cycle over the
//! port-handover seam: connect → Ready → EraseDeviceFlash → BlankFlash →
//! FlashFirmware → Ready → ResetRuntime → Ready. This is the manual driver
//! until the `lp device matrix` runner subcommand lands.
//!
//! ```sh
//! just studio-firmware-package-esp32c6
//! cargo run -p lpa-link --features host-serial-esp32 --example manage_smoke -- \
//!     /dev/cu.usbmodem101 target/studio-web-assets/firmware/esp32c6/manifest.json
//! ```
//!
//! DESTRUCTIVE: erases the connected device's flash, then reflashes the
//! packaged firmware.

use std::rc::Rc;
use std::time::Duration;

use lpa_link::providers::host_serial_esp32::{
    HostSerialEsp32Options, HostSerialEsp32Provider, label_for_port,
};
use lpa_link::{
    DeviceDeadlines, DeviceEvent, DeviceEventSink, DeviceSession, DeviceState, DeviceTimers,
    LinkConnector, LinkManagementRequest,
};

fn event_printer() -> DeviceEventSink {
    DeviceEventSink::new(|event| match event {
        DeviceEvent::LogLine { line, .. } => println!("  | {line}"),
        DeviceEvent::State { state } => println!("  * state: {state:?}"),
        DeviceEvent::Progress { label, percent } => match percent {
            Some(percent) => println!("  % {label}: {percent}%"),
            None => println!("  % {label}"),
        },
    })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let port = args
        .next()
        .expect("usage: manage_smoke <port> <manifest.json>");
    let manifest = args
        .next()
        .expect("usage: manage_smoke <port> <manifest.json>");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(run(port, manifest)));
}

async fn run(port: String, manifest: String) {
    let provider = HostSerialEsp32Provider::with_options(HostSerialEsp32Options {
        reset_after_open: true,
        firmware_manifest_path: Some(manifest),
        ..HostSerialEsp32Options::default()
    });
    let endpoint_id = provider.create_endpoint_for_port(&port, label_for_port(&port));
    let connector = Rc::new(LinkConnector::HostSerialEsp32(provider));
    let timers = DeviceTimers::new(|duration| Box::pin(tokio::time::sleep(duration)))
        .with_deadlines(DeviceDeadlines {
            ready: Duration::from_secs(30),
            ..DeviceDeadlines::default()
        });

    println!("== connect ==");
    let session = DeviceSession::connect(connector, &endpoint_id, timers, event_printer())
        .await
        .expect("device session connect");
    let state = session.wait_ready().await;
    println!("connected: {state:?}");
    assert!(
        state.is_ready(),
        "expected Ready after connect, got {state:?}"
    );

    println!("\n== manage: EraseDeviceFlash (expect BlankFlash after rebuild) ==");
    let outcome = session
        .manage(LinkManagementRequest::EraseDeviceFlash, event_printer())
        .await
        .expect("erase device flash");
    println!("erase outcome state: {:?}", outcome.state);
    assert!(
        matches!(outcome.state, DeviceState::BlankFlash),
        "expected BlankFlash after erase, got {:?}",
        outcome.state
    );

    println!("\n== manage: FlashFirmware (expect Ready after rebuild) ==");
    let outcome = session
        .manage(LinkManagementRequest::FlashFirmware, event_printer())
        .await
        .expect("flash firmware");
    println!("flash outcome state: {:?}", outcome.state);
    assert!(
        outcome.state.is_ready(),
        "expected Ready after flash, got {:?}",
        outcome.state
    );

    println!("\n== manage: ResetRuntime (expect Ready after rebuild) ==");
    let outcome = session
        .manage(LinkManagementRequest::ResetRuntime, event_printer())
        .await
        .expect("reset runtime");
    println!("reset outcome state: {:?}", outcome.state);
    assert!(
        outcome.state.is_ready(),
        "expected Ready after reset, got {:?}",
        outcome.state
    );

    session.close().await.expect("close session");
    println!("\nMANAGE SMOKE: PASS");
}
